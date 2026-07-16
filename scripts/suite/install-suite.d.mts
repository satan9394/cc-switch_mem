export interface ConfigurationBackup {
  directory: string;
  entries: Array<{ source: string; backup: string }>;
}

export function buildInstallArgs(packageRoot: string): string[];
export function sanitizeLog(value: unknown): string;
export function backupConfiguration(files?: string[]): ConfigurationBackup;
export function restoreConfiguration(backup: ConfigurationBackup): void;
export function runSuiteInstall(root: string): number;
