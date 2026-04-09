import { useState } from "react";
import type { DateConstraint, ConstraintKind } from "../../../protocol";

interface Props {
  value: DateConstraint | null;
  onChange: (c: DateConstraint | null) => void;
}

export function ConstraintEditor({ value, onChange }: Props) {
  const [kind, setKind] = useState<ConstraintKind | "None">(value?.kind ?? "None");
  const [date, setDate] = useState(value?.date ?? "");

  const update = (k: ConstraintKind | "None", d: string) => {
    setKind(k);
    setDate(d);
    if (k === "None") {
      onChange(null);
    } else if (d) {
      onChange({ kind: k, date: d });
    }
  };

  return (
    <div className="form-row">
      <label>Constraint</label>
      <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
        {(["None", "Earliest", "Fixed", "Latest"] as const).map((k) => (
          <label
            key={k}
            style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 13, cursor: "pointer", color: "#d4d4d4" }}
          >
            <input
              type="radio"
              name="constraint-kind"
              checked={kind === k}
              onChange={() => update(k, date)}
            />
            {k}
          </label>
        ))}
        {kind !== "None" && (
          <input
            type="date"
            value={date}
            onChange={(e) => update(kind, e.target.value)}
            style={{
              background: "#1e1e1e",
              border: "1px solid #3a3a3c",
              borderRadius: 4,
              color: "#d4d4d4",
              fontSize: 13,
              padding: "4px 8px",
              outline: "none",
            }}
          />
        )}
      </div>
    </div>
  );
}
