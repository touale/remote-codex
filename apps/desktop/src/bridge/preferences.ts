export interface AppPreferences {
  revision: number;
  theme: 'system' | 'light' | 'dark';
  five_hour_limit: boolean;
  weekly_limit: boolean;
  context_usage: boolean;
  session_tokens: boolean;
}
export type AppPreferencesPatch = Partial<Omit<AppPreferences, 'revision'>>;
export const defaultAppPreferences: AppPreferences = {
  revision: 0,
  theme: 'system',
  five_hour_limit: true,
  weekly_limit: false,
  context_usage: true,
  session_tokens: false,
};
