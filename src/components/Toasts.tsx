import { dismiss, toasts } from "../stores/notices";
import { useStore } from "../stores/store";
import { IconX } from "./Icons";

export function Toasts() {
  const list = useStore(toasts);
  return (
    <div className="toasts" role="status" aria-live="polite">
      {list.map((t) => (
        <div key={t.id} className={"toast " + t.level}>
          <div className="toast-text">
            <div>{t.text}</div>
            {t.detail ? <div className="toast-detail">{t.detail}</div> : null}
          </div>
          <button className="icon-btn small" onClick={() => dismiss(t.id)} title="閉じる">
            <IconX />
          </button>
        </div>
      ))}
    </div>
  );
}
