// Bottom-right toast stack (design_system: errors persist, info auto-dismiss).

import { useToastStore } from "../stores/toasts";
import { focusRing } from "./ui";

export function Toasts() {
  const { toasts, dismiss } = useToastStore();
  if (toasts.length === 0) return null;
  return (
    <div className="fixed bottom-4 right-4 z-50 flex w-80 flex-col gap-2">
      {toasts.map((toast) => (
        <button
          key={toast.id}
          type="button"
          onClick={() => dismiss(toast.id)}
          className={`rounded-2xl px-4 py-3 text-left text-body shadow-pop transition ${
            toast.kind === "error"
              ? "bg-surface text-danger ring-1 ring-danger/40"
              : "bg-surface text-text"
          } ${focusRing}`}
        >
          {toast.message}
        </button>
      ))}
    </div>
  );
}
