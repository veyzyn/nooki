interface IconProps {
  size?: number;
  className?: string;
  color?: string;
}

export const IconGrid = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <rect x="1" y="1" width="6" height="6" rx="1.5" fill="currentColor" opacity=".8"/>
    <rect x="9" y="1" width="6" height="6" rx="1.5" fill="currentColor" opacity=".8"/>
    <rect x="1" y="9" width="6" height="6" rx="1.5" fill="currentColor" opacity=".8"/>
    <rect x="9" y="9" width="6" height="6" rx="1.5" fill="currentColor" opacity=".8"/>
  </svg>
);

export const IconServer = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <rect x="1" y="2" width="14" height="4" rx="1.5" stroke="currentColor" strokeWidth="1.3"/>
    <rect x="1" y="10" width="14" height="4" rx="1.5" stroke="currentColor" strokeWidth="1.3"/>
    <circle cx="12" cy="4" r="1" fill="currentColor"/>
    <circle cx="12" cy="12" r="1" fill="currentColor"/>
    <circle cx="9.5" cy="4" r="1" fill="currentColor" opacity=".5"/>
    <circle cx="9.5" cy="12" r="1" fill="currentColor" opacity=".5"/>
  </svg>
);

export const IconBox = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M8 1.5L14 4.5V11.5L8 14.5L2 11.5V4.5L8 1.5Z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/>
    <path d="M2 4.5L8 7.5L14 4.5" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M8 7.5V14.5" stroke="currentColor" strokeWidth="1.3"/>
  </svg>
);

export const IconShare = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <circle cx="12.5" cy="3.5" r="1.75" stroke="currentColor" strokeWidth="1.3"/>
    <circle cx="3.5" cy="8" r="1.75" stroke="currentColor" strokeWidth="1.3"/>
    <circle cx="12.5" cy="12.5" r="1.75" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M5.2 7.1L10.8 4.4M5.2 8.9L10.8 11.6" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
  </svg>
);

export const IconSettings = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <circle cx="8" cy="8" r="2.5" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M8 1.5V3M8 13V14.5M1.5 8H3M13 8H14.5M3.08 3.08L4.14 4.14M11.86 11.86L12.92 12.92M3.08 12.92L4.14 11.86M11.86 4.14L12.92 3.08" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
  </svg>
);

export const IconPlay = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M4.5 3.5L13 8L4.5 12.5V3.5Z" fill="currentColor"/>
  </svg>
);

export const IconStop = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <rect x="3.5" y="3.5" width="9" height="9" rx="1.5" fill="currentColor"/>
  </svg>
);

export const IconRefresh = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M13.5 8A5.5 5.5 0 1 1 8 2.5c1.8 0 3.4.87 4.4 2.2" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
    <path d="M10.5 2.5L12.5 4.5L14.5 2.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/>
  </svg>
);

export const IconDots = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <circle cx="8" cy="3" r="1.25" fill="currentColor"/>
    <circle cx="8" cy="8" r="1.25" fill="currentColor"/>
    <circle cx="8" cy="13" r="1.25" fill="currentColor"/>
  </svg>
);

export const IconPlus = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M8 2.5V13.5M2.5 8H13.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
  </svg>
);

export const IconChevronRight = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M6 3.5L10.5 8L6 12.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/>
  </svg>
);

export const IconChevronLeft = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M10 3.5L5.5 8L10 12.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/>
  </svg>
);

export const IconChevronDown = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M3.5 6L8 10.5L12.5 6" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/>
  </svg>
);

export const IconCheck = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M2.5 8.5L6.5 12.5L13.5 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
  </svg>
);

export const IconWarning = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M8 1.5L14.5 13.5H1.5L8 1.5Z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/>
    <path d="M8 6V9" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
    <circle cx="8" cy="11.5" r="0.75" fill="currentColor"/>
  </svg>
);

export const IconInfo = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <circle cx="8" cy="8" r="6.5" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M8 7.5V11" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
    <circle cx="8" cy="5" r="0.75" fill="currentColor"/>
  </svg>
);

export const IconX = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M3.5 3.5L12.5 12.5M12.5 3.5L3.5 12.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
  </svg>
);

export const IconSearch = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M10.5 10.5L13.5 13.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
  </svg>
);

export const IconDownload = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M8 2V10M5 7.5L8 10.5L11 7.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/>
    <path d="M2.5 12H13.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
  </svg>
);

export const IconPlug = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M5.25 2.25V5M10.75 2.25V5M3.5 5H12.5V6.25C12.5 8.46 10.71 10.25 8.5 10.25H7.5C5.29 10.25 3.5 8.46 3.5 6.25V5Z" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/>
    <path d="M8 10.25V14" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
  </svg>
);

export const IconMod = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path
      d="M6 2H3.5C2.67 2 2 2.67 2 3.5V6H3.25C4.22 6 5 6.78 5 7.75S4.22 9.5 3.25 9.5H2V12.5C2 13.33 2.67 14 3.5 14H6.5V12.75C6.5 11.78 7.28 11 8.25 11S10 11.78 10 12.75V14H12.5C13.33 14 14 13.33 14 12.5V9.5H12.75C11.78 9.5 11 8.72 11 7.75S11.78 6 12.75 6H14V3.5C14 2.67 13.33 2 12.5 2H10V3.25C10 4.22 9.22 5 8.25 5S6.5 4.22 6.5 3.25V2H6Z"
      stroke="currentColor"
      strokeWidth="1.25"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

export const IconTrash = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M2.5 4H13.5M6 1.75H10L10.75 4H5.25L6 1.75Z" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/>
    <path d="M4 4L4.65 13.25C4.7 13.82 5.17 14.25 5.74 14.25H10.26C10.83 14.25 11.3 13.82 11.35 13.25L12 4M6.5 6.5V11.75M9.5 6.5V11.75" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
  </svg>
);

export const IconUpload = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M8 10V2M5 4.5L8 1.5L11 4.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/>
    <path d="M2.5 12H13.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
  </svg>
);

export const IconFolder = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M1.5 4.5C1.5 3.67 2.17 3 3 3H6L7.5 4.5H13C13.83 4.5 14.5 5.17 14.5 6V12C14.5 12.83 13.83 13.5 13 13.5H3C2.17 13.5 1.5 12.83 1.5 12V4.5Z" stroke="currentColor" strokeWidth="1.3"/>
  </svg>
);

export const IconTerminal = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M3 5L6.5 8L3 11" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/>
    <path d="M8.5 11H13" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
    <rect x="1.5" y="1.5" width="13" height="13" rx="2" stroke="currentColor" strokeWidth="1.3"/>
  </svg>
);

export const IconUsers = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <circle cx="6" cy="5.5" r="2.5" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M1.5 14C1.5 11.51 3.51 9.5 6 9.5C8.49 9.5 10.5 11.51 10.5 14" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
    <path d="M10.5 4C11.88 4 13 5.12 13 6.5C13 7.88 11.88 9 10.5 9" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
    <path d="M12 12.5C13.11 12.76 14 13.52 14.5 14.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
  </svg>
);

export const IconFileText = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M9.5 1.5H3.5C2.67 1.5 2 2.17 2 3V13C2 13.83 2.67 14.5 3.5 14.5H12.5C13.33 14.5 14 13.83 14 13V6L9.5 1.5Z" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M9.5 1.5V6H14" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/>
    <path d="M5 9H11M5 11.5H9" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
  </svg>
);

export const IconCpu = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <rect x="4" y="4" width="8" height="8" rx="1" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M6 1.5V4M8 1.5V4M10 1.5V4M6 12V14.5M8 12V14.5M10 12V14.5M1.5 6H4M1.5 8H4M1.5 10H4M12 6H14.5M12 8H14.5M12 10H14.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/>
  </svg>
);

export const IconHardDrive = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <rect x="1.5" y="4.5" width="13" height="7" rx="2" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M1.5 9H14.5" stroke="currentColor" strokeWidth="1.3"/>
    <circle cx="11.5" cy="11.5" r="0.75" fill="currentColor"/>
    <circle cx="9" cy="11.5" r="0.75" fill="currentColor"/>
  </svg>
);

export const IconDatabase = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <ellipse cx="8" cy="3.5" rx="5.5" ry="2" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M2.5 3.5V8.1C2.5 9.2 4.96 10.1 8 10.1C11.04 10.1 13.5 9.2 13.5 8.1V3.5" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M2.5 8V12.5C2.5 13.6 4.96 14.5 8 14.5C11.04 14.5 13.5 13.6 13.5 12.5V8" stroke="currentColor" strokeWidth="1.3"/>
  </svg>
);

export const IconMemory = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <rect x="1.5" y="5" width="13" height="6" rx="1" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M4 5V3.5M6.5 5V3.5M9 5V3.5M11.5 5V3.5M4 11V12.5M6.5 11V12.5M9 11V12.5M11.5 11V12.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/>
    <rect x="4" y="6.5" width="2" height="3" rx="0.5" fill="currentColor" opacity=".7"/>
    <rect x="9" y="6.5" width="2" height="3" rx="0.5" fill="currentColor" opacity=".7"/>
  </svg>
);

export const IconCloud = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M12.5 10.5H4.5C2.84 10.5 1.5 9.16 1.5 7.5C1.5 5.84 2.84 4.5 4.5 4.5C4.5 3.12 5.62 2 7 2C8.1 2 9.04 2.7 9.4 3.67C9.76 3.55 10.15 3.5 10.5 3.5C12.16 3.5 13.5 4.84 13.5 6.5C14.33 6.5 14.5 7.17 14.5 7.5C14.5 9.16 13.66 10.5 12.5 10.5Z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/>
  </svg>
);

export const IconArrowLeft = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M12.5 8H3.5M3.5 8L7 4.5M3.5 8L7 11.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/>
  </svg>
);

export const IconCopy = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <rect x="5" y="5" width="9" height="9" rx="1.5" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M3 11H2.5C1.67 11 1 10.33 1 9.5V2.5C1 1.67 1.67 1 2.5 1H9.5C10.33 1 11 1.67 11 2.5V3" stroke="currentColor" strokeWidth="1.3"/>
  </svg>
);

export const IconZap = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M9.5 1.5L3 9H8.5L6.5 14.5L14 6.5H8.5L9.5 1.5Z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/>
  </svg>
);

export const IconShield = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M8 1.5L2.5 4V8C2.5 11.17 4.92 14.08 8 14.5C11.08 14.08 13.5 11.17 13.5 8V4L8 1.5Z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/>
    <path d="M5.5 8L7 9.5L10.5 6" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/>
  </svg>
);

export const IconSliders = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M2 4H10M10 4C10 5.1 10.9 6 12 6C13.1 6 14 5.1 14 4C14 2.9 13.1 2 12 2C10.9 2 10 2.9 10 4Z" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
    <path d="M14 12H6M6 12C6 13.1 5.1 14 4 14C2.9 14 2 13.1 2 12C2 10.9 2.9 10 4 10C5.1 10 6 10.9 6 12Z" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
  </svg>
);

export const IconGlobe = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <circle cx="8" cy="8" r="6.5" stroke="currentColor" strokeWidth="1.3"/>
    <ellipse cx="8" cy="8" rx="2.5" ry="6.5" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M1.5 8H14.5M2 5H14M2 11H14" stroke="currentColor" strokeWidth="1.1" strokeLinecap="round"/>
  </svg>
);

export const IconFeather = ({ size = 16, className = '' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" className={className}>
    <path d="M13.8 2.2C10.8 1.4 7.2 2.5 5.1 4.6C3.3 6.4 2.5 8.8 2.4 11.2L1 14.6L4.4 13.2C6.8 13.1 9.2 12.3 11 10.5C13.1 8.4 14.2 4.9 13.8 2.2Z" stroke="currentColor" strokeWidth="1.25" strokeLinejoin="round"/>
    <path d="M3 12.8L10.8 5M5.3 10.5H8.8M7.1 8.7V5.8" stroke="currentColor" strokeWidth="1.15" strokeLinecap="round"/>
  </svg>
);

// Minecraft-inspired block icon for server avatars
export const IconBlock = ({ size = 32, className = '', color = '#5fb87f' }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 32 32" fill="none" className={className}>
    <rect x="4" y="4" width="24" height="24" rx="4" fill={color} opacity=".18"/>
    <rect x="4" y="4" width="24" height="24" rx="4" stroke={color} strokeWidth="1.5" opacity=".4"/>
    {/* Grass layer */}
    <rect x="4" y="4" width="24" height="8" rx="4" fill={color} opacity=".35"/>
    <rect x="4" y="9" width="24" height="3" fill={color} opacity=".2"/>
    {/* Dirt detail dots */}
    <rect x="9" y="16" width="3" height="3" rx="0.5" fill="currentColor" opacity=".2"/>
    <rect x="15" y="19" width="3" height="3" rx="0.5" fill="currentColor" opacity=".15"/>
    <rect x="20" y="15" width="3" height="3" rx="0.5" fill="currentColor" opacity=".18"/>
  </svg>
);

export const NookiLogo = ({ size = 28 }: { size?: number }) => (
  <img
    src={new URL('../assets/nooki-logo.svg', import.meta.url).href}
    width={size}
    height={size}
    alt=""
    aria-hidden="true"
    draggable={false}
  />
);
