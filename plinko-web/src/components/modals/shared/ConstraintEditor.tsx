import { useState } from "react";
import type { DateConstraint, ConstraintKind } from "../../../protocol";
import { SegmentedControl } from "./SegmentedControl";
import { DatePicker } from "./DatePicker";

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
        <DatePicker
          value={date}
          onChange={(d) => update(kind, d)}
          disabled={kind === "None"}
          placeholder="Select date…"
        />
      </div>
    </div>
  );
}
