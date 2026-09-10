/**
 * UI notice that can never crash the host.
 *
 * `ctx.ui` throws once the session has been replaced (`ctx.reload()`,
 * `newSession`, `fork`, `switchSession`), and background workers routinely
 * outlive the session that spawned them — a reload while a worker runs is the
 * normal case, not the edge case. A notice is best-effort UI sugar; the
 * durable record is the worker log, so a stale context is swallowed rather
 * than escalated into an uncaught exception that takes pi down.
 */
export function notify(ctx: unknown, message: string, level: "info" | "warning" = "info"): void {
  try {
    const ui = (ctx as { ui?: { notify?: (m: string, l?: string) => void } } | undefined)?.ui;
    ui?.notify?.(message, level);
  } catch {
    // Stale context after a session swap — the worker log holds the detail.
  }
}
