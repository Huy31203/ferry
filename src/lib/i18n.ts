import en from './i18n/en.json';

export type StringKey = keyof typeof en;

/** UI string for `key`, with `{name}` placeholders filled from `vars`. */
export function t(key: StringKey, vars: Record<string, string | number> = {}): string {
  return en[key].replace(/\{(\w+)\}/g, (match, name: string) =>
    name in vars ? String(vars[name]) : match,
  );
}
