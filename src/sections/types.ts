import type { Settings, Status } from "../api";

export interface SectionProps {
  settings: Settings;
  update: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  status: Status;
  refreshStatus: () => Promise<void>;
  flash: (text: string, error?: boolean) => void;
}
