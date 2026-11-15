// Formatos de hora y duración ("22:30", "3 h 06 min", "18 min").

export const hhmm = (ms: number): string =>
  new Date(ms).toLocaleTimeString('es', { hour: '2-digit', minute: '2-digit', hour12: false });

export const minutesUntil = (ms: number, now: number): number => Math.max(0, Math.ceil((ms - now) / 60_000));

export const secondsUntil = (ms: number, now: number): number => Math.max(0, Math.ceil((ms - now) / 1000));

export function duration(minutes: number): string {
  if (minutes < 60) return `${minutes} min`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m === 0 ? `${h} h` : `${h} h ${String(m).padStart(2, '0')} min`;
}

export const today = (): string =>
  new Date().toLocaleDateString('es', { weekday: 'long', day: 'numeric', month: 'long' });

/** "HH:MM" ⇄ minutos desde medianoche, para <input type="time">. */
export const toMinutes = (t: string): number => {
  const [h, m] = t.split(':').map(Number);
  return (h || 0) * 60 + (m || 0);
};
export const fromMinutes = (m: number): string =>
  `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
