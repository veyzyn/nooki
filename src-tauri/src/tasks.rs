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
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let host = sample_host();
            *state.host.write().await = host.clone();
            state.emit(AppEvent::HostMetrics(host));
            let mut system = sysinfo::System::new_all();
            system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
            let ids = state
                .servers
                .read()
                .await
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            for id in ids {
                let Some(pid) = state.processes.pid(&id).await else {
                    if tick.is_multiple_of(30) {
                        if let Ok(mut server) = state.server(&id).await {
                            server.disk_used =
                                crate::state::directory_size(std::path::Path::new(&server.folder))
                                    .await;
                            state
                                .servers
                                .write()
                                .await
                                .insert(id.clone(), server.clone());
                            state.emit(AppEvent::ServerChanged(server));
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
                let cpu = tree
                    .iter()
                    .filter_map(|process_id| system.process(*process_id))
                    .map(|process| process.cpu_usage())
                    .sum::<f32>();
                let memory = tree
                    .iter()
                    .filter_map(|process_id| system.process(*process_id))
                    .map(|process| process.memory())
                    .sum::<u64>();
                if let Ok(mut server) = state.server(&id).await {
                    server.cpu = cpu;
                    server.memory = memory as f64 / 1_048_576.0;
                    let sample_at = now_ms();
                    let sample_due = server
                        .history
                        .last()
                        .is_none_or(|sample| sample_at.saturating_sub(sample.at) >= 60_000);
                    if sample_due {
                        server.disk_used =
                            crate::state::directory_size(std::path::Path::new(&server.folder))
                                .await;
                        let memory_pct = if server.max_memory == 0 {
                            0.0
                        } else {
                            (server.memory / server.max_memory as f64 * 100.0) as f32
                        };
                        server.history.push(ResourceSample {
                            at: sample_at,
                            cpu: server.cpu,
                            memory: memory_pct,
                            players: server.players,
                        });
                        // Keep the complete session time range without allowing very long-running
                        // servers to grow IPC snapshots forever. Once the chart gets dense, halve
                        // its resolution; step_by retains both the session's first and newest point.
                        if server.history.len() > 2_048 {
                            server.history = server.history.iter().step_by(2).cloned().collect();
                        }
                    }
                    state
                        .servers
                        .write()
                        .await
                        .insert(id.clone(), server.clone());
                    state.emit(AppEvent::ServerChanged(server));
                }
            }
            tick = tick.wrapping_add(1);
        }
    });
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
