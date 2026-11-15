// Cliente del servicio. La UI no decide nada: muestra lo que dice el servicio y le pide acciones.
import { agentRequest } from './native';

export type DayState =
  | { state: 'sleep' }
  | { state: 'locked' }
  | { state: 'pass'; until_ms: number }
  | { state: 'focus'; goal_id: string }
  | { state: 'free' }
  | { state: 'emergency'; until_ms: number };

export type GoalKind = 'key' | 'habit';
export type Method =
  | 'focus_session' | 'qr_anchor' | 'health' | 'honor_declaration' | 'witness'
  | 'food_photo' | 'reading_presence' | 'page_ocr' | 'summary' | 'pose_reps';

export interface Goal {
  id: string;
  title: string;
  kind: GoalKind;
  target_minutes: number;
  target_count: number;
  methods: Method[];
  weekdays: number;
}

export interface GoalProgress {
  goal_id: string;
  title: string;
  kind: GoalKind;
  done_count: number;
  target_count: number;
  focus_minutes: number;
  target_minutes: number;
  completed: boolean;
}

export interface Status {
  state: DayState;
  permits_left: number;
  day_start_ms: number;
  next_wake_ms: number;
  sleep_at_ms: number | null;
  free_day: boolean;
  needs_plan: boolean;
  goals: GoalProgress[];
}

export interface AgentStatus {
  core: Status | null;
  blocked_app: string | null;
  blocked_at_ms: number | null;
  grace_until_ms: number | null;
  synced_at_ms: number | null;
}

export interface Config {
  schedule: { wake: { minutes: number }; bedtime: { minutes: number } };
  permits_per_day: number;
  permit_minutes: number;
  permit_cooldown_minutes: number;
  emergency_minutes: number;
  emergency_penalty: number;
  habit_penalty: boolean;
  goals: Goal[];
  free_days: number;
  essential_apps: string[];
  focus_apps: string[];
  web_allowlist: string[];
  /** Permisos extra semanales del tutor (modo familiar). Se omite si es 0. */
  weekly_bonus_permits?: number;
}

// ── Modo familiar (este PC es de un menor; el tutor usa el teléfono) ──
export type Role = 'owner' | 'guardian' | 'minor' | 'partner';
export type RequestKind =
  | { what: 'extra_permit'; minutes: number }
  | { what: 'add_site'; domain: string }
  | { what: 'later_bedtime'; minutes: number }
  | { what: 'rest_day' };
export interface PendingRequest { request_id: string; what: RequestKind; at_ms: number }
export type TamperKind =
  | 'permission_revoked' | 'service_stopped' | 'clock_changed' | 'uninstall_attempt' | 'safe_mode' | 'device_silent';
export type FamilyAlert =
  | { alert: 'tamper'; what: TamperKind }
  | { alert: 'emergency' }
  | { alert: 'blocked_sites'; count: number };
export interface FamilyAlertView { at_ms: number; alert: FamilyAlert; device_label: string }

type Response =
  | { type: 'status'; agent: AgentStatus }
  | { type: 'ok' }
  | { type: 'text'; value: string }
  | { type: 'maybe_text'; value: string | null }
  | { type: 'effective'; from_ms: number }
  | { type: 'config'; config: Config | null }
  | { type: 'goals'; goals: Goal[] }
  | { type: 'members'; members: unknown[] }
  | { type: 'devices'; devices: DeviceSeen[] }
  | { type: 'role'; role: Role | null }
  | { type: 'requests'; requests: PendingRequest[] }
  | { type: 'alerts'; alerts: FamilyAlertView[] }
  | { type: 'error'; message: string; cooldown_minutes: number };

export async function call(request: Record<string, unknown>): Promise<Response> {
  const r = await agentRequest<Response>(request);
  if (r.type === 'error') throw new Error(r.message);
  return r;
}

export async function status(): Promise<AgentStatus> {
  const r = await call({ cmd: 'status' });
  if (r.type !== 'status') throw new Error('respuesta inesperada');
  return r.agent;
}

export async function myRole(): Promise<Role | null> {
  const r = await call({ cmd: 'my_role' });
  return r.type === 'role' ? r.role : null;
}

export async function familyView(): Promise<{ requests: PendingRequest[]; alerts: FamilyAlertView[] }> {
  const [r, a] = await Promise.all([call({ cmd: 'pending_requests' }), call({ cmd: 'family_alerts' })]);
  return { requests: r.type === 'requests' ? r.requests : [], alerts: a.type === 'alerts' ? a.alerts : [] };
}

export async function todayGoals(): Promise<Goal[]> {
  const r = await call({ cmd: 'today_goals' });
  return r.type === 'goals' ? r.goals : [];
}

export async function text(request: Record<string, unknown>): Promise<string | null> {
  const r = await call(request);
  return r.type === 'text' || r.type === 'maybe_text' ? r.value : null;
}

export interface DeviceSeen {
  device_id: string;
  label: string;
  last_seen_ms: number;
  is_me: boolean;
}

export async function devices(): Promise<DeviceSeen[]> {
  const r = await call({ cmd: 'devices_seen' });
  return r.type === 'devices' ? r.devices : [];
}

/** Cómo se comprueba una meta. Siempre hay una opción sin cámara ("marcar como hecha"). */
export type Check = 'enfoque' | 'lectura' | 'ejercicio' | 'comida' | 'paginas' | 'marcar';
export const CHECKS: [Check, string][] = [
  ['enfoque', 'Sesión de enfoque'],
  ['lectura', 'Cámara: lectura'],
  ['ejercicio', 'Cámara: ejercicio (20 repeticiones)'],
  ['comida', 'Foto de comida (en el teléfono)'],
  ['paginas', 'Páginas + resumen (en el teléfono)'],
  ['marcar', 'Marcar como hecha'],
];
/** Métodos que se pueden comprobar con la cámara del PC. */
export const PC_CAMERA: Method[] = ['reading_presence', 'pose_reps'];

export function methodsForCheck(check: Check, minutes: number): Method[] {
  switch (check) {
    case 'lectura': return ['reading_presence', 'focus_session', 'honor_declaration'];
    case 'ejercicio': return ['pose_reps', 'honor_declaration'];
    case 'comida': return ['food_photo', 'honor_declaration'];
    case 'paginas': return ['page_ocr', 'summary', 'honor_declaration'];
    case 'marcar': return ['honor_declaration'];
    default: return methodsFor(minutes);
  }
}

/** Las comprobaciones por conteo (ejercicio, comida) no llevan minutos. */
export const usesMinutes = (check: Check): boolean => check === 'enfoque' || check === 'lectura';

export const ALL_DAYS = 0b111_1111;
export const DAY_LABELS = ['L', 'M', 'X', 'J', 'V', 'S', 'D'];
export const DAY_NAMES = ['lunes', 'martes', 'miércoles', 'jueves', 'viernes', 'sábado', 'domingo'];

/** Métodos que acepta una meta según se mida por tiempo o no (Fase 1: enfoque o declaración). */
export function methodsFor(minutes: number): Method[] {
  return minutes > 0 ? ['focus_session', 'honor_declaration'] : ['honor_declaration'];
}

/** Identificador estable a partir del título: minúsculas, sin tildes, con guiones. */
export function slug(title: string, taken: string[]): string {
  const base =
    title.toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '')
      .replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 24) || 'meta';
  let id = base;
  for (let n = 2; taken.includes(id); n++) id = `${base.slice(0, 20)}-${n}`;
  return id;
}
