// Shared UI primitives (design_system.md). Components never restyle these
// locally; variants are added here.

import { type CSSProperties, type ReactNode, useEffect, useRef, useState } from "react";

/** Visible keyboard-focus ring (a11y), shared by every interactive control. */
export const focusRing =
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-mask focus-visible:ring-offset-2 focus-visible:ring-offset-ink";

export function Card({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={`flex flex-col gap-4 rounded-2xl bg-surface p-5 shadow-card ${className ?? ""}`}
    >
      {children}
    </div>
  );
}

export function PanelHeader({ title, action }: { title: string; action?: ReactNode }) {
  return (
    <div className="flex items-center justify-between">
      <h2 className="text-subheading font-semibold text-text">{title}</h2>
      {action}
    </div>
  );
}

export function pill(active: boolean): string {
  return [
    "rounded-full px-3 py-1 text-body font-semibold transition-colors",
    focusRing,
    active ? "bg-mask text-ink" : "bg-raised text-text-dim hover:bg-overlay",
  ].join(" ");
}

type ButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement>;

export function PrimaryButton({ className, type, ...rest }: ButtonProps) {
  return (
    <button
      type={type === "submit" ? "submit" : "button"}
      className={`rounded-lg bg-mask px-5 py-2.5 text-body-lg font-semibold text-ink transition hover:bg-mask-strong disabled:opacity-50 ${focusRing} ${className ?? ""}`}
      {...rest}
    />
  );
}

export function GhostButton({ className, type, ...rest }: ButtonProps) {
  return (
    <button
      type={type === "submit" ? "submit" : "button"}
      className={`rounded-lg bg-raised px-3 py-1.5 text-body font-semibold text-text-dim ring-1 ring-outline transition hover:bg-overlay hover:text-text disabled:opacity-50 ${focusRing} ${className ?? ""}`}
      {...rest}
    />
  );
}

/** Destructive action with inline double-press confirmation (no OS dialogs). */
export function DangerButton({
  label,
  confirmLabel,
  onConfirm,
  className,
}: {
  label: string;
  confirmLabel: string;
  onConfirm: () => void;
  className?: string;
}) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const timer = setTimeout(() => setArmed(false), 3000);
    return () => clearTimeout(timer);
  }, [armed]);
  return (
    <button
      type="button"
      onClick={() => {
        if (armed) {
          setArmed(false);
          onConfirm();
        } else {
          setArmed(true);
        }
      }}
      className={`rounded-lg px-3 py-1.5 text-body font-semibold ring-1 transition ${
        armed
          ? "bg-danger text-ink ring-danger"
          : "bg-transparent text-danger ring-danger/40 hover:ring-danger"
      } ${focusRing} ${className ?? ""}`}
    >
      {armed ? confirmLabel : label}
    </button>
  );
}

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div>
      <p className="mb-1.5 text-body font-semibold uppercase tracking-wide text-text-dim">
        {label}
      </p>
      {children}
    </div>
  );
}

export function Select({
  value,
  onChange,
  children,
  disabled,
}: {
  value: string;
  onChange: (v: string) => void;
  children: ReactNode;
  disabled?: boolean;
}) {
  return (
    <div className="relative">
      <select
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
        className={`w-full appearance-none rounded-lg bg-raised px-3 py-2 pr-9 text-body text-text ring-1 ring-outline disabled:opacity-50 ${focusRing}`}
      >
        {children}
      </select>
      <span
        aria-hidden
        className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-text-dim"
      >
        ▾
      </span>
    </div>
  );
}

/** Range slider with mask-colored fill. `bipolar` snaps to 0 near center. */
export function Slider({
  min,
  max,
  value,
  onChange,
  step = 1,
  bipolar = false,
}: {
  min: number;
  max: number;
  value: number;
  onChange: (v: number) => void;
  step?: number;
  bipolar?: boolean;
}) {
  const pct = max > min ? ((value - min) / (max - min)) * 100 : 0;
  return (
    <input
      type="range"
      min={min}
      max={max}
      step={step}
      value={value}
      onChange={(e) => {
        let v = Number(e.target.value);
        if (bipolar && Math.abs(v) < step) v = 0;
        onChange(v);
      }}
      className="mask-range"
      style={{ "--fill": `${pct}%` } as CSSProperties}
    />
  );
}

/** Horizontal level meter driven by dBFS, with rAF decay and a peak-hold tick.
 *  live → warn above -12 dBFS → danger at clip (design_system). */
export function Meter({ db, label }: { db: number; label: string }) {
  const barRef = useRef<HTMLDivElement>(null);
  const peakRef = useRef<HTMLDivElement>(null);
  const state = useRef({ level: 0, peak: 0, peakAt: 0, target: 0 });

  state.current.target = dbToFraction(db);

  useEffect(() => {
    let raf = 0;
    const tick = (now: number) => {
      const s = state.current;
      // Rise instantly, decay smoothly (design_system: meters run on rAF).
      s.level = s.target > s.level ? s.target : Math.max(s.target, s.level - 0.025);
      if (s.level >= s.peak || now - s.peakAt > 300) {
        s.peak = s.level;
        s.peakAt = now;
      }
      if (barRef.current) {
        barRef.current.style.width = `${s.level * 100}%`;
        barRef.current.style.backgroundColor =
          s.level > 0.98
            ? "var(--color-danger)"
            : s.level > dbToFraction(-12)
              ? "var(--color-warn)"
              : "var(--color-live)";
      }
      if (peakRef.current) {
        peakRef.current.style.left = `${s.peak * 100}%`;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <div className="flex items-center gap-2">
      <span className="w-8 shrink-0 text-body text-text-dim">{label}</span>
      <div className="relative h-2 grow overflow-hidden rounded-full bg-overlay">
        <div ref={barRef} className="h-full rounded-full" style={{ width: "0%" }} />
        <div
          ref={peakRef}
          className="absolute top-0 h-full w-px bg-text-dim"
          style={{ left: "0%" }}
        />
      </div>
    </div>
  );
}

function dbToFraction(db: number): number {
  // Map -60..0 dBFS to 0..1.
  return Math.min(1, Math.max(0, (db + 60) / 60));
}

export type StatusKind = "stopped" | "starting" | "running" | "error";

export function StatusDot({ status }: { status: StatusKind }) {
  const color = {
    stopped: "bg-text-faint",
    starting: "bg-warn mask-pulse",
    running: "bg-live",
    error: "bg-danger",
  }[status];
  return <span className={`inline-block h-2.5 w-2.5 rounded-full ${color}`} />;
}

export function Spinner() {
  return (
    <span className="inline-block h-5 w-5 shrink-0 animate-spin rounded-full border-2 border-mask/30 border-t-mask" />
  );
}

export function EmptyState({
  title,
  hint,
  action,
}: {
  title: string;
  hint: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex grow flex-col items-center justify-center gap-2 py-10 text-center">
      <p className="text-body-lg font-semibold text-text-dim">{title}</p>
      <p className="text-body text-text-faint">{hint}</p>
      {action}
    </div>
  );
}

/** Mono-font numeric readout (ms, dB, st) so digits do not jitter. */
export function MonoValue({ children }: { children: ReactNode }) {
  return <span className="font-mono text-body text-text-dim">{children}</span>;
}
