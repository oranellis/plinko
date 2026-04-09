/** Inline SVG icon components. All icons render at 16×16 by default. */

interface IconProps {
  size?: number;
  color?: string;
  style?: React.CSSProperties;
}

const defaults = { size: 16, color: "currentColor" };

export function IconBack({ size = defaults.size, color = defaults.color }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <polyline points="9,3 4,8 9,13" stroke={color} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function IconGantt({ size = defaults.size, color = defaults.color }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <rect x="2" y="3" width="7" height="2.5" rx="1" fill={color} />
      <rect x="5" y="6.75" width="6" height="2.5" rx="1" fill={color} />
      <rect x="3" y="10.5" width="8" height="2.5" rx="1" fill={color} />
    </svg>
  );
}

export function IconAllocation({ size = defaults.size, color = defaults.color }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <rect x="2" y="2" width="3" height="7" rx="1" fill={color} />
      <rect x="6.5" y="2" width="3" height="11" rx="1" fill={color} />
      <rect x="11" y="2" width="3" height="5" rx="1" fill={color} />
    </svg>
  );
}

export function IconCalendar({ size = defaults.size, color = defaults.color }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <rect x="2" y="3" width="12" height="11" rx="1.5" stroke={color} strokeWidth="1.4" />
      <line x1="2" y1="7" x2="14" y2="7" stroke={color} strokeWidth="1.2" />
      <line x1="5.5" y1="1.5" x2="5.5" y2="5" stroke={color} strokeWidth="1.4" strokeLinecap="round" />
      <line x1="10.5" y1="1.5" x2="10.5" y2="5" stroke={color} strokeWidth="1.4" strokeLinecap="round" />
      <rect x="4.5" y="9" width="2" height="2" rx="0.4" fill={color} />
      <rect x="7" y="9" width="2" height="2" rx="0.4" fill={color} />
      <rect x="9.5" y="9" width="2" height="2" rx="0.4" fill={color} />
    </svg>
  );
}

export function IconSettings({ size = defaults.size, color = defaults.color }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <circle cx="8" cy="8" r="2.3" stroke={color} strokeWidth="1.4" />
      <path
        d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4"
        stroke={color} strokeWidth="1.4" strokeLinecap="round"
      />
    </svg>
  );
}

export function IconUsers({ size = defaults.size, color = defaults.color }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <circle cx="6" cy="5.5" r="2.3" stroke={color} strokeWidth="1.3" />
      <path d="M1.5 13.5c0-2.5 2-4 4.5-4s4.5 1.5 4.5 4" stroke={color} strokeWidth="1.3" strokeLinecap="round" />
      <circle cx="11.5" cy="5" r="1.8" stroke={color} strokeWidth="1.2" />
      <path d="M11 9.5c1.5 0 3 1 3 3" stroke={color} strokeWidth="1.2" strokeLinecap="round" />
    </svg>
  );
}

export function IconSearch({ size = defaults.size, color = defaults.color }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <circle cx="6.5" cy="6.5" r="4" stroke={color} strokeWidth="1.4" />
      <line x1="9.8" y1="9.8" x2="13.5" y2="13.5" stroke={color} strokeWidth="1.6" strokeLinecap="round" />
    </svg>
  );
}

export function IconToday({ size = defaults.size, color = defaults.color }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <rect x="2" y="3" width="12" height="11" rx="1.5" stroke={color} strokeWidth="1.3" />
      <line x1="2" y1="7" x2="14" y2="7" stroke={color} strokeWidth="1.2" />
      <line x1="5.5" y1="1.5" x2="5.5" y2="5" stroke={color} strokeWidth="1.4" strokeLinecap="round" />
      <line x1="10.5" y1="1.5" x2="10.5" y2="5" stroke={color} strokeWidth="1.4" strokeLinecap="round" />
      <line x1="8" y1="9" x2="8" y2="13" stroke={color} strokeWidth="1.4" strokeLinecap="round" />
      <line x1="5.5" y1="11" x2="10.5" y2="11" stroke={color} strokeWidth="1.4" strokeLinecap="round" />
    </svg>
  );
}

export function IconAddTask({ size = defaults.size, color = defaults.color }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <rect x="1.5" y="4" width="9" height="2.5" rx="1" fill={color} />
      <rect x="1.5" y="9.5" width="6" height="2.5" rx="1" fill={color} />
      <line x1="12" y1="9" x2="12" y2="15" stroke={color} strokeWidth="1.6" strokeLinecap="round" />
      <line x1="9" y1="12" x2="15" y2="12" stroke={color} strokeWidth="1.6" strokeLinecap="round" />
    </svg>
  );
}

export function IconAddMilestone({ size = defaults.size, color = defaults.color }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path d="M7 2.5L2.5 7L7 11.5L11.5 7Z" stroke={color} strokeWidth="1.4" strokeLinejoin="round" />
      <line x1="13" y1="10" x2="13" y2="15.5" stroke={color} strokeWidth="1.5" strokeLinecap="round" />
      <line x1="10.25" y1="12.75" x2="15.75" y2="12.75" stroke={color} strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}

export function IconHome({ size = defaults.size, color = defaults.color }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path d="M2 8L8 2.5L14 8" stroke={color} strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M4 6.5V13.5H12V6.5" stroke={color} strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
      <rect x="6" y="9.5" width="4" height="4" rx="0.5" fill={color} />
    </svg>
  );
}
