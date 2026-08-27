import { describe, grouped } from "../app/shortcuts";

interface Props {
  open: boolean;
  onClose: () => void;
}

/** Keyboard reference, so every action is discoverable without a mouse. */
export function ShortcutsHelp({ open, onClose }: Props) {
  if (!open) return null;

  return (
    <div className="modal-backdrop" role="presentation">
      <section className="modal shortcuts-modal" role="dialog" aria-modal="true" aria-label="快捷键速查">
        <header className="panel-head">
          <h2>快捷键</h2>
          <button type="button" className="toolbar-button" onClick={onClose} aria-label="关闭">
            ✕
          </button>
        </header>

        <div className="modal-body shortcuts-grid">
          {grouped().map(({ group, items }) => (
            <section key={group} className="shortcut-group">
              <h3>{group}</h3>
              <dl>
                {items.map((shortcut) => (
                  <div key={shortcut.id} className="shortcut-row">
                    <dt>{shortcut.label}</dt>
                    <dd>
                      <kbd>{describe(shortcut)}</kbd>
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
          ))}
        </div>

        <footer className="modal-foot">
          <button type="button" className="primary" onClick={onClose}>
            知道了
          </button>
        </footer>
      </section>
    </div>
  );
}
