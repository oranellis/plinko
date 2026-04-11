import React, { useEffect, useRef } from "react";
import "./Modal.css";

interface ModalProps {
  title: string;
  onClose: () => void;
  children: React.ReactNode;
  width?: number | string;
  /** When false, clicking backdrop does not close (default: true). */
  closeOnBackdrop?: boolean;
  /**
   * When provided, pressing Enter (while not focused in a textarea or
   * contenteditable element) will call this handler — equivalent to clicking
   * the primary save/confirm button.
   */
  onSave?: () => void;
}

export function Modal({
  title,
  onClose,
  children,
  width = 520,
  closeOnBackdrop = true,
  onSave,
}: ModalProps) {
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        return;
      }
      if (e.key === "Enter" && onSave) {
        const active = document.activeElement;
        const isMultiline =
          active instanceof HTMLTextAreaElement ||
          (active instanceof HTMLElement && active.isContentEditable);
        if (!isMultiline) {
          e.preventDefault();
          onSave();
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose, onSave]);

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(e) => {
        if (closeOnBackdrop && e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="modal-panel"
        ref={panelRef}
        style={{ width }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="modal-header">
          <span className="modal-title">{title}</span>
          <button className="modal-close" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>
        <div className="modal-body">{children}</div>
      </div>
    </div>
  );
}
