// Bridge to the native app hosting this UI: Tauri on Windows, WKWebView on macOS.
// The rest of the UI does not know which platform it runs on.
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

/** Handlers registered by the macOS app (`WKScriptMessageHandlerWithReply`). */
type MacHandler = { postMessage(body: string): Promise<string> };
const mac = (window as unknown as { webkit?: { messageHandlers?: Record<string, MacHandler> } }).webkit?.messageHandlers;

export const isMac = !!mac?.agent;

const macCall = (name: 'agent' | 'native', body: unknown): Promise<string> => mac![name].postMessage(JSON.stringify(body));

/** A request to the privileged service (the same JSON lines on both platforms). */
export async function agentRequest<T>(request: Record<string, unknown>): Promise<T> {
  return isMac ? (JSON.parse(await macCall('agent', request)) as T) : invoke<T>('agent', { request });
}

export const lockScreen = (): Promise<unknown> => (isMac ? macCall('native', { cmd: 'lock_screen' }) : invoke('lock_screen'));

export const qrSvg = (text: string): Promise<string> =>
  isMac ? macCall('native', { cmd: 'qr_svg', text }) : invoke<string>('qr_svg', { text });

/** Full-screen, always-on-top shield (on) or a normal window (off). */
export async function setShield(on: boolean): Promise<void> {
  if (isMac) {
    await macCall('native', { cmd: 'shield', on });
    return;
  }
  const w = getCurrentWindow();
  await w.setAlwaysOnTop(on);
  await w.setFullscreen(on);
  if (on) {
    await w.show();
    await w.setFocus();
  }
}

export async function bringToFront(): Promise<void> {
  if (isMac) {
    await macCall('native', { cmd: 'show' });
    return;
  }
  const w = getCurrentWindow();
  await w.show();
  await w.setFocus();
}

/** Verification with the PC camera (AppContainer without network): Windows only for now. */
export const canVerifyOnPc = !isMac;
export const verifyOnPc = (goalId: string, method: string, targetMinutes: number): Promise<boolean> =>
  invoke<boolean>('verify_on_pc', { goalId, method, targetMinutes });
export const stopVerification = (): Promise<unknown> => invoke('stop_verification');
