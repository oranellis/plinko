import { useEffect, useRef, useState } from "react";

interface Props extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "type"> {
  value: number;
  onChange: (value: number) => void;
}

/**
 * A controlled number input that allows the field to be temporarily empty
 * while the user is typing. An empty field is interpreted as 0.
 */
export function NumberInput({ value, onChange, ...props }: Props) {
  const [str, setStr] = useState(() => value === 0 ? "" : String(value));
  // Track the last externally-set value so we can detect genuine external changes.
  const lastExternal = useRef(value);

  useEffect(() => {
    if (value !== lastExternal.current) {
      lastExternal.current = value;
      // Only overwrite the string if the current string doesn't represent the
      // same number (e.g. don't reset "0." to "" while the user is mid-type).
      const current = parseFloat(str);
      const currentNum = isNaN(current) ? 0 : current;
      if (currentNum !== value) {
        setStr(value === 0 ? "" : String(value));
      }
    }
  }, [value, str]);

  return (
    <input
      type="number"
      {...props}
      value={str}
      onChange={(e) => {
        const s = e.target.value;
        setStr(s);
        const num = parseFloat(s);
        onChange(isNaN(num) ? 0 : num);
      }}
    />
  );
}
