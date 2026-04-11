import { useEffect, useRef, useState } from "react";

interface Props extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "type"> {
  value: number;
  onChange: (value: number) => void;
  /** If set, values above this trigger an error style */
  max?: number;
}

/**
 * A controlled number input that:
 * - Only allows digits and a single decimal point
 * - Allows the field to be temporarily empty (interpreted as 0)
 * - Shows an error style when the value exceeds `max`
 */
export function NumberInput({ value, onChange, max, style, ...props }: Props) {
  const [str, setStr] = useState(() => value === 0 ? "" : String(value));
  const lastExternal = useRef(value);

  useEffect(() => {
    if (value !== lastExternal.current) {
      lastExternal.current = value;
      const current = parseFloat(str);
      const currentNum = isNaN(current) ? 0 : current;
      if (currentNum !== value) {
        setStr(value === 0 ? "" : String(value));
      }
    }
  }, [value, str]);

  const num = parseFloat(str);
  const isOver = max !== undefined && !isNaN(num) && num > max;

  return (
    <input
      type="text"
      inputMode="decimal"
      {...props}
      style={isOver ? { ...style, borderColor: "#e05c5c", color: "#e05c5c" } : style}
      value={str}
      onKeyDown={(e) => {
        // Allow: digits, dot, backspace, delete, arrows, tab, home, end
        const allowed = /^[0-9.]$/.test(e.key);
        const control = ["Backspace", "Delete", "ArrowLeft", "ArrowRight", "Tab", "Home", "End"].includes(e.key);
        const isCtrl = e.ctrlKey || e.metaKey;
        if (!allowed && !control && !isCtrl) e.preventDefault();
        // Prevent second dot
        if (e.key === "." && str.includes(".")) e.preventDefault();
        props.onKeyDown?.(e);
      }}
      onChange={(e) => {
        const s = e.target.value.replace(/[^0-9.]/g, "").replace(/(\..*)\./g, "$1");
        setStr(s);
        const n = parseFloat(s);
        onChange(isNaN(n) ? 0 : n);
      }}
    />
  );
}
