export {
  LayoutDashboard as IconGrid,
  Server as IconServer,
  Archive as IconBox,
  Share2 as IconShare,
  Settings as IconSettings,
  Play as IconPlay,
  Square as IconStop,
  RefreshCw as IconRefresh,
  EllipsisVertical as IconDots,
  Plus as IconPlus,
  ChevronRight as IconChevronRight,
  ChevronLeft as IconChevronLeft,
  ChevronDown as IconChevronDown,
  Check as IconCheck,
  TriangleAlert as IconWarning,
  Info as IconInfo,
  X as IconX,
  Search as IconSearch,
  Download as IconDownload,
  Plug as IconPlug,
  Puzzle as IconMod,
  Trash2 as IconTrash,
  Upload as IconUpload,
  Folder as IconFolder,
  FolderOpen as IconFolderOpen,
  FilePlus2 as IconFilePlus,
  FolderPlus as IconFolderPlus,
  Save as IconSave,
  Pencil as IconPencil,
  ExternalLink as IconExternalLink,
  Bold as IconBold,
  Italic as IconItalic,
  Underline as IconUnderline,
  Strikethrough as IconStrikethrough,
  Dices as IconObfuscated,
  RemoveFormatting as IconRemoveFormatting,
  AlignLeft as IconAlignLeft,
  AlignCenter as IconAlignCenter,
  AlignRight as IconAlignRight,
  SquareTerminal as IconTerminal,
  Users as IconUsers,
  FileText as IconFileText,
  Cpu as IconCpu,
  HardDrive as IconHardDrive,
  Database as IconDatabase,
  MemoryStick as IconMemory,
  Cloud as IconCloud,
  ArrowLeft as IconArrowLeft,
  Copy as IconCopy,
  Zap as IconZap,
  ShieldCheck as IconShield,
  SlidersHorizontal as IconSliders,
  Globe2 as IconGlobe,
  Feather as IconFeather,
  Box as IconBlock,
} from 'lucide-react';

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
