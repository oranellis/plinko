/**
 * Segmented button control matching the Rust UI draw_segmented style.
 * Selected segment: blue (#4a90d9). Unselected: #3a3a3c.
 * All segments connected with vertical dividers, outer rounded border.
 */
interface Props {
  options: string[];
  selected: number;
  onChange: (idx: number) => void;
  small?: boolean;
}

export function SegmentedControl({ options, selected, onChange, small = false }: Props) {
  const fontSize = small ? 12 : 13;
  const height = small ? 26 : 30;

  return (
    <div style={{
      display: "flex",
      border: "1px solid #3a3a3c",
      borderRadius: 4,
      overflow: "hidden",
      height,
    }}>
      {options.map((label, i) => {
        const isSel = i === selected;
        const isLast = i === options.length - 1;
        return (
          <button
            key={i}
            onClick={() => onChange(i)}
            style={{
              flex: 1,
              background: isSel ? "#4a90d9" : "#3a3a3c",
              border: "none",
              borderRight: isLast ? "none" : "1px solid #2a2a2c",
              color: isSel ? "#fff" : "#d4d4d4",
              fontSize,
              fontWeight: isSel ? 600 : 400,
              cursor: "pointer",
              fontFamily: "inherit",
              padding: "0 4px",
              transition: "background 0.1s",
            }}
            onMouseEnter={(e) => { if (!isSel) e.currentTarget.style.background = "#4a4a4c"; }}
            onMouseLeave={(e) => { if (!isSel) e.currentTarget.style.background = "#3a3a3c"; }}
          >
            {label}
          </button>
        );
      })}
    </div>
  );
}
