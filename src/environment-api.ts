import { command } from "./api";

export type EnvironmentScope = "user" | "system";
export type EnvironmentValueKind = "string" | "expandString" | "unsupported";
export interface EnvironmentVariable {
  name: string;
  value: string;
  kind: EnvironmentValueKind;
  revision: string;
}
export interface EnvironmentScopeSnapshot {
  scope: EnvironmentScope;
  variables: EnvironmentVariable[];
  writable: boolean;
  error: string | null;
}
export interface EnvironmentSnapshot {
  supported: boolean;
  scopes: EnvironmentScopeSnapshot[];
}
export interface EnvironmentChange {
  scope: EnvironmentScope;
  name: string;
  value: string | null;
  kind: EnvironmentValueKind;
  expectedRevision: string | null;
}
export interface EnvironmentChangeResult {
  variable: EnvironmentVariable | null;
  warning: string | null;
}
export const environmentApi = {
  list: () => command<EnvironmentSnapshot>("get_environment_variables"),
  change: (change: EnvironmentChange) =>
    command<EnvironmentChangeResult>("change_environment_variable", { change }),
};
