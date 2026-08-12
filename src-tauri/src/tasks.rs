use std::{sync::Arc, time::Duration};

use crate::{
    backups,
    commands::next_schedule_time,
    models::{now_ms, AppEvent, ResourceSample, ServerStatus},
    state::{sample_host, AppState},
};

pub fn spawn_background_tasks(state: Arc<AppState>) {
    spawn_metrics(state.clone());
    spawn_scheduler(state.clone());
    spawn_update_checks(state);
}

fn spawn_metrics(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut tick = 0u32;
        // sysinfo calculates process CPU from the difference between refreshes.
        // Keep one process table alive so readings after the first tick are real.
        let mut system = sysinfo::System::new_all();
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if tick.is_multiple_of(2) {
                let host = sample_host();
                *state.host.write().await = host.clone();
                state.emit(AppEvent::HostMetrics(host));
            }
            system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
            let logical_cpu_count = system.cpus().len().max(1);
            let ids = state
                .servers
                .read()
                .await
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            for id in ids {
                let Some(pid) = state.processes.pid(&id).await else {
                    if tick.is_multiple_of(60) {
                        if let Ok(server) = state.server(&id).await {
                            let disk_used =
                                crate::state::directory_size(std::path::Path::new(&server.folder))
                                    .await;
                            let changed = {
                                let mut servers = state.servers.write().await;
                                servers.get_mut(&id).map(|current| {
                                    current.disk_used = disk_used;
                                    current.clone()
                                })
                            };
                            if let Some(changed) = changed {
                                state.emit(AppEvent::ServerChanged(changed));
                            }
                        }
                    }
                    continue;
                };
                let root_pid = sysinfo::Pid::from_u32(pid);
                if system.process(root_pid).is_none() {
                    continue;
                }
                let mut tree = vec![root_pid];
                loop {
                    let children = system
                        .processes()
                        .iter()
                        .filter_map(|(candidate, process)| {
                            process
                                .parent()
                                .filter(|parent| tree.contains(parent) && !tree.contains(candidate))
                                .map(|_| *candidate)
                        })
                        .collect::<Vec<_>>();
                    if children.is_empty() {
                        break;
                    }
                    tree.extend(children);
                }
                let raw_cpu = tree
                    .iter()
                    .filter_map(|process_id| system.process(*process_id))
                    .map(|process| process.cpu_usage())
                    .sum::<f32>();
                let cpu = normalize_process_cpu(raw_cpu, logical_cpu_count);
                let memory = tree
                    .iter()
                    .filter_map(|process_id| system.process(*process_id))
                    .map(|process| process.memory())
                    .sum::<u64>();
                if let Ok(server) = state.server(&id).await {
                    let sample_at = now_ms();
                    let disk_used = if tick.is_multiple_of(60) {
                        Some(
                            crate::state::directory_size(std::path::Path::new(&server.folder))
                                .await,
                        )
                    } else {
                        None
                    };
                    if !state.processes.is_running(&id).await {
                        continue;
                    }
                    let metrics = {
                        let mut servers = state.servers.write().await;
                        servers.get_mut(&id).map(|current| {
                            current.cpu = cpu;
                            current.memory = memory as f64 / 1_048_576.0;
                            if let Some(disk_used) = disk_used {
                                current.disk_used = disk_used;
                            }
                            let memory_pct = if current.max_memory == 0 {
                                0.0
                            } else {
                                (current.memory / current.max_memory as f64 * 100.0) as f32
                            };
                            let sample = ResourceSample {
                                at: sample_at,
                                cpu: current.cpu,
                                memory: memory_pct,
                                players: current.players,
                            };
                            let cutoff = sample_at.saturating_sub(3_600_000);
                            current.history.retain(|item| item.at >= cutoff);
                            current.history.push(sample.clone());
                            (current.memory, current.disk_used, sample)
                        })
                    };
                    if let Some((memory, disk_used, sample)) = metrics {
                        state.emit(AppEvent::ServerMetrics {
                            server_id: id.clone(),
                            cpu,
                            memory,
                            disk_used,
                            sample,
                        });
                    }
                }
            }
            tick = tick.wrapping_add(1);
        }
    });
}

fn normalize_process_cpu(raw_cpu: f32, logical_cpu_count: usize) -> f32 {
    (raw_cpu / logical_cpu_count.max(1) as f32).clamp(0.0, 100.0)
}

#[cfg(test)]
mod metric_tests {
    use super::normalize_process_cpu;

    #[test]
    fn process_cpu_is_normalized_to_whole_machine_usage() {
        assert_eq!(normalize_process_cpu(800.0, 16), 50.0);
        assert_eq!(normalize_process_cpu(20.0, 0), 20.0);
        assert_eq!(normalize_process_cpu(2_000.0, 8), 100.0);
    }
}

fn spawn_scheduler(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        loop {
            let schedules = state.schedules.read().await.clone();
            for (server_id, mut schedule) in schedules {
                if !schedule.enabled {
                    continue;
                }
                let due = schedule.next_run_at.is_some_and(|next| next <= now_ms());
                if schedule.next_run_at.is_none() {
                    schedule.next_run_at = next_schedule_time(&schedule).ok();
                    let _ = state.db.save_schedule(&server_id, &schedule).await;
                    state
                        .schedules
                        .write()
                        .await
                        .insert(server_id.clone(), schedule.clone());
                    state.emit(AppEvent::ScheduleChanged {
                        server_id: server_id.clone(),
                        schedule,
                    });
                    continue;
                }
                if !due {
                    continue;
                }
                let lock = state.operation_lock(&server_id);
                let Ok(_guard) = lock.try_lock() else {
                    continue;
                };
                if backups::create_backup(
                    state.clone(),
                    &server_id,
                    "scheduled",
                    Some("Automatic backup".into()),
                    None,
                )
                .await
                .is_ok()
                {
                    schedule.last_run_at = Some(now_ms());
                }
                schedule.next_run_at = next_schedule_time(&schedule).ok();
                let _ = state.db.save_schedule(&server_id, &schedule).await;
                state
                    .schedules
                    .write()
                    .await
                    .insert(server_id.clone(), schedule.clone());
                state.emit(AppEvent::ScheduleChanged {
                    server_id,
                    schedule,
                });
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

fn spawn_update_checks(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        loop {
            let ids = state
                .servers
                .read()
                .await
                .values()
                .filter(|server| {
                    server.server_type == crate::models::ServerType::Paper
                        && server.status != ServerStatus::Updating
                })
                .map(|server| server.id.clone())
                .collect::<Vec<_>>();
            for id in ids {
                let Ok(mut server) = state.server(&id).await else {
                    continue;
                };
                if let Ok(resolved) = state
                    .catalog
                    .resolve(
                        crate::models::ServerType::Paper,
                        &server.version,
                        None,
                        false,
                    )
                    .await
                {
                    if resolved.build != server.build {
                        server.update_available = Some(crate::models::UpdateAvailable {
                            version: server.version.clone(), build: resolved.build,
                            notes: "A newer stable Paper build is available for this Minecraft version.".into(), experimental: false,
                        });
                        let _ = state.save_server(server.clone()).await;
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(24 * 60 * 60)).await;
        }
    });
}
