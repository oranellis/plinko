import { useState } from "react";
import type React from "react";
import type { DateConstraint, ConstraintKind } from "../../../protocol";
import { SegmentedControl } from "./SegmentedControl";

const KIND_OPTIONS = ["None", "Earliest", "Fixed", "Latest"] as const;
type KindOrNone = ConstraintKind | "None";

interface Props {
  value: DateConstraint | null;
  onChange: (c: DateConstraint | null) => void;
}

export function ConstraintEditor({ value, onChange }: Props) {
  const [kind, setKind] = useState<KindOrNone>(value?.kind ?? "None");
  const [date, setDate] = useState(value?.date ?? "");

  const update = (k: KindOrNone, d: string) => {
    setKind(k);
    setDate(d);
    if (k === "None") {
      onChange(null);
    } else if (d) {
      onChange({ kind: k, date: d });
    } else {
      onChange(null);
    }
  };

  const kindIdx = KIND_OPTIONS.indexOf(kind);

  const inputStyle: React.CSSProperties = {
    background: kind === "None" ? "#1a1a1c" : "#1e1e1e",
    border: "1px solid #3a3a3c",
    borderRadius: 4,
    color: kind === "None" ? "#555" : "#d4d4d4",
    fontSize: 13,
    padding: "0 10px",
    outline: "none",
    width: "100%",
    boxSizing: "border-box",
    height: 30,
    cursor: kind === "None" ? "not-allowed" : "auto",
  };

  return (
    <div style={{ display: "flex", gap: 12, marginBottom: 16 }}>
      {/* Left column: Constraint Type */}
      <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 6 }}>
        <label style={{ fontSize: 12, color: "#999", textTransform: "uppercase", letterSpacing: "0.04em" }}>
          Constraint Type
        </label>
        <SegmentedControl
          options={["None", "Earliest", "Fixed", "Latest"]}
          selected={kindIdx}
          onChange={(i) => update(KIND_OPTIONS[i], date)}
        />
      </div>

      {/* Right column: Constraint Date */}
      <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 6 }}>
        <label style={{ fontSize: 12, color: "#999", textTransform: "uppercase", letterSpacing: "0.04em" }}>
          Constraint Date
        </label>
        <input
          type="date"
          value={date}
          disabled={kind === "None"}
          onChange={(e) => update(kind, e.target.value)}
          style={inputStyle}
        />
      </div>
    </div>
  );
}
