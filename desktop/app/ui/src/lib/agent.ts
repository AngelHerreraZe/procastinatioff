// Service client. The UI decides nothing: it shows what the service says and asks it for actions.
import { agentRequest } from './native';

export type DayState =
  | { state: 'sleep' }
  | { state: 'locked' }
  | { state: 'pass'; until_ms: number }
  | { state: 'focus'; goal_id: string; apps: string[]; sites: string[] }
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
  /** What this goal's focus opens besides the essential apps (omitted when empty). */
  apps?: string[];
  sites?: string[];
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
  /** Weekly extra passes from the guardian (family mode). Omitted when 0. */
  weekly_bonus_permits?: number;
}

// ── Family mode (this PC belongs to a minor; the guardian uses the phone) ──
export type Role = 'owner' | 'guardian' | 'minor' | 'trusted_contact';
export type RequestKind =
  | { what: 'extra_permit'; minutes: number }
  | { what: 'add_site'; domain: string }
  | { what: 'later_bedtime'; minutes: number }
  | { what: 'rest_day' }
  | { what: 'config_change'; changes: unknown[]; config_hash: string }; // only in a trusted contact's channel
export interface PendingRequest { request_id: string; what: RequestKind; at_ms: number }
export type TamperKind =
  | 'permission_revoked' | 'service_stopped' | 'clock_changed' | 'uninstall_attempt' | 'safe_mode' | 'device_silent';
export type FamilyAlert =
  | { alert: 'tamper'; what: TamperKind }
  | { alert: 'emergency' }
  | { alert: 'blocked_sites'; count: number }
  | { alert: 'granted'; what: OfflineAsk };
/** A small concession granted with an offline code or the guardian's PIN in person. */
export type OfflineAsk =
  | { ask: 'extra_permit'; minutes: number }
  | { ask: 'rest_day' }
  | { ask: 'later_bedtime'; minutes: number };
export interface FamilyAlertView { at_ms: number; alert: FamilyAlert; device_label: string }

// ── Authorizing changes (ADR 0013): the local gate on this computer ──
export type LocalFactor = 'pin' | 'password' | 'pattern' | 'biometric';
export interface AuthState {
  methods: LocalFactor[];
  /** Those usable now (biometrics: only on the phone for now). */
  usable: LocalFactor[];
  guardian_methods: LocalFactor[];
  locked_until_ms: number;
  reset_at_ms: number | null;
  has_recovery_code: boolean;
  contact_required: boolean;
  contacts: number;
  unlocked: boolean;
}

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
  | { type: 'auth'; state: AuthState }
  | { type: 'error'; code: string; message: string; cooldown_minutes: number };

/** Error from the service. `code` is a translation key (`denied.*`, `error.*`): see `errorText` in i18n.ts. */
export class AgentError extends Error {
  constructor(
    readonly code: string,
    readonly cooldown_minutes: number,
    message: string,
  ) {
    super(message);
  }
}

export async function call(request: Record<string, unknown>): Promise<Response> {
  const r = await agentRequest<Response>(request);
  if (r.type === 'error') throw new AgentError(r.code, r.cooldown_minutes, r.message);
  return r;
}

export async function status(): Promise<AgentStatus> {
  const r = await call({ cmd: 'status' });
  if (r.type !== 'status') throw new Error('unexpected response');
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

export async function authState(): Promise<AuthState | null> {
  const r = await call({ cmd: 'auth_state' });
  return r.type === 'auth' ? r.state : null;
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

/** How a goal is checked (label: `check.<value>` in the locale files). There is always a camera-free option ("mark"). */
export type Check = 'focus' | 'reading' | 'exercise' | 'food' | 'pages' | 'mark';
export const CHECKS: Check[] = ['focus', 'reading', 'exercise', 'food', 'pages', 'mark'];
/** Methods that can be checked with the PC camera. */
export const PC_CAMERA: Method[] = ['reading_presence', 'pose_reps'];

export function methodsForCheck(check: Check, minutes: number): Method[] {
  switch (check) {
    case 'reading': return ['reading_presence', 'focus_session', 'honor_declaration'];
    case 'exercise': return ['pose_reps', 'honor_declaration'];
    case 'food': return ['food_photo', 'honor_declaration'];
    case 'pages': return ['page_ocr', 'summary', 'honor_declaration'];
    case 'mark': return ['honor_declaration'];
    default: return methodsFor(minutes);
  }
}

/** Count-based checks (exercise, food) have no minutes. */
export const usesMinutes = (check: Check): boolean => check === 'focus' || check === 'reading';

export const ALL_DAYS = 0b111_1111;

/** Methods a goal accepts depending on whether it is measured by time (focus or declaration). */
export function methodsFor(minutes: number): Method[] {
  return minutes > 0 ? ['focus_session', 'honor_declaration'] : ['honor_declaration'];
}

/** Stable identifier from the title: lowercase, no accents, hyphens. */
export function slug(title: string, taken: string[]): string {
  const base =
    title.toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '')
      .replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 24) || 'goal';
  let id = base;
  for (let n = 2; taken.includes(id); n++) id = `${base.slice(0, 20)}-${n}`;
  return id;
}
