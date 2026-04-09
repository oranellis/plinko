import React, { useEffect, useRef } from "react";
import "./Modal.css";

interface ModalProps {
  title: string;
  onClose: () => void;
  children: React.ReactNode;
  width?: number | string;
  /** When false, clicking backdrop does not close (default: true). */
  closeOnBackdrop?: boolean;
}

export function Modal({
  title,
  onClose,
  children,
  width = 520,
  closeOnBackdrop = true,
}: ModalProps) {
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

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
