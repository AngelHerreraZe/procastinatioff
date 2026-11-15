// Puente con la app nativa que aloja esta UI: Tauri en Windows, WKWebView en macOS (08b · 3.6).
// Lo demás de la UI no sabe en qué plataforma está.
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

/** Manejadores que registra la app de macOS (`WKScriptMessageHandlerWithReply`). */
type MacHandler = { postMessage(body: string): Promise<string> };
const mac = (window as unknown as { webkit?: { messageHandlers?: Record<string, MacHandler> } }).webkit?.messageHandlers;

export const isMac = !!mac?.agent;

const macCall = (name: 'agent' | 'native', body: unknown): Promise<string> => mac![name].postMessage(JSON.stringify(body));

/** Una petición al servicio privilegiado (mismas líneas JSON en las dos plataformas). */
export async function agentRequest<T>(request: Record<string, unknown>): Promise<T> {
  return isMac ? (JSON.parse(await macCall('agent', request)) as T) : invoke<T>('agent', { request });
}

export const lockScreen = (): Promise<unknown> => (isMac ? macCall('native', { cmd: 'lock_screen' }) : invoke('lock_screen'));

export const qrSvg = (text: string): Promise<string> =>
  isMac ? macCall('native', { cmd: 'qr_svg', text }) : invoke<string>('qr_svg', { text });

/** Escudo a pantalla completa y siempre encima (on) o ventana normal (off). */
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

/** Verificación con la cámara del PC (AppContainer sin red): solo en Windows por ahora. */
export const canVerifyOnPc = !isMac;
export const verifyOnPc = (goalId: string, method: string, targetMinutes: number): Promise<boolean> =>
  invoke<boolean>('verify_on_pc', { goalId, method, targetMinutes });
export const stopVerification = (): Promise<unknown> => invoke('stop_verification');
