import type { CSSProperties } from 'react';
import type { Server, ServerType } from '../types';
import paperLogo from '../assets/papermc-logo.svg';
import minecraftLogo from '../assets/minecraft-grass-block.svg';
import forgeLogo from '../assets/minecraftforge-anvil.svg';
import fabricLogo from '../assets/fabricmc-logo.svg';
import neoforgeLogo from '../assets/neoforged-logo.svg';
import './ServerIcon.css';

const softwareLogos: Record<ServerType, string> = {
  vanilla: minecraftLogo,
  paper: paperLogo,
  forge: forgeLogo,
  neoforge: neoforgeLogo,
  fabric: fabricLogo,
};

export function SoftwareIcon({ type, size = 22 }: { type: ServerType; size?: number }) {
  return <img className={`software-brand-logo is-${type}`} src={softwareLogos[type]} alt="" style={{ '--software-logo-size': `${size}px` } as CSSProperties} />;
}

export default function ServerIcon({ server, size = 40 }: { server: Pick<Server, 'name' | 'type' | 'iconData'>; size?: number }) {
  return (
    <span className={`server-avatar ${server.iconData ? 'is-custom' : `is-${server.type}`}`} style={{ '--server-avatar-size': `${size}px` } as CSSProperties}>
      <img src={server.iconData ?? softwareLogos[server.type]} alt={`${server.name} icon`} />
    </span>
  );
}
