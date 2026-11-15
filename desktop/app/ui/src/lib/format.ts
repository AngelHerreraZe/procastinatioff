// Time and duration formats in the user's language ("22:30" or "10:30 PM", "3 h 06 min", "18 min").
import { locale, t } from './i18n';

export const hhmm = (ms: number): string =>
  new Date(ms).toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' });

export const minutesUntil = (ms: number, now: number): number => Math.max(0, Math.ceil((ms - now) / 60_000));

export const secondsUntil = (ms: number, now: number): number => Math.max(0, Math.ceil((ms - now) / 1000));

export function duration(minutes: number): string {
  if (minutes < 60) return t('duration.min', { n: minutes });
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m === 0 ? t('duration.h', { h }) : t('duration.h_min', { h, m: String(m).padStart(2, '0') });
}

export const today = (): string =>
  new Date().toLocaleDateString(locale, { weekday: 'long', day: 'numeric', month: 'long' });

/** "HH:MM" ⇄ minutes since midnight, for <input type="time">. */
export const toMinutes = (t: string): number => {
  const [h, m] = t.split(':').map(Number);
  return (h || 0) * 60 + (m || 0);
};
export const fromMinutes = (m: number): string =>
  `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
