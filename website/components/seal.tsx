import { useId, type SVGProps } from "react";
import { MARK_PATH } from "@/components/brand";

const mid = (p: number[], q: number[]) => [(p[0] + q[0]) / 2, (p[1] + q[1]) / 2];
const fmt = (p: number[]) => `${p[0].toFixed(2)} ${p[1].toFixed(2)}`;

// A wax blob: 20 soft lobes around r≈47, deterministic so server and client agree.
const BLOB = (() => {
  const n = 20;
  const pts = Array.from({ length: n }, (_, i) => {
    const a = (i / n) * Math.PI * 2;
    const r = 46 + (i % 2 ? 2.6 : 0) + ((i * 7) % 5) * 0.35;
    return [50 + r * Math.cos(a), 50 + r * Math.sin(a)];
  });
  let d = `M${fmt(mid(pts[n - 1], pts[0]))}`;
  pts.forEach((p, i) => (d += ` Q${fmt(p)} ${fmt(mid(p, pts[(i + 1) % n]))}`));
  return `${d}Z`;
})();

/**
 * The compiler's seal: wax pressed over a signature it has checked.
 * `label` runs around the rim; the mark is the brand check, embossed.
 */
export function Seal({
  label = "signature checked · promise kept · ",
  ...props
}: SVGProps<SVGSVGElement> & { label?: string }) {
  const rim = `seal-rim-${useId()}`;
  return (
    <svg viewBox="0 0 100 100" aria-hidden="true" {...props}>
      <defs>
        <path id={rim} d="M50 50m-36.5 0a36.5 36.5 0 1 1 73 0a36.5 36.5 0 1 1-73 0" />
      </defs>
      <path d={BLOB} fill="var(--seal)" />
      <path d={BLOB} fill="none" stroke="var(--seal-ink)" strokeOpacity="0.18" strokeWidth="1.2" />
      <circle cx="50" cy="50" r="30" fill="none" stroke="var(--seal-ink)" strokeOpacity="0.32" />
      <circle cx="50" cy="50" r="43" fill="none" stroke="var(--seal-ink)" strokeOpacity="0.14" />
      <text
        fill="var(--seal-ink)"
        fillOpacity="0.62"
        fontSize="7.4"
        letterSpacing="2.05"
        className="font-mono uppercase"
      >
        <textPath href={`#${rim}`}>{label}</textPath>
      </text>
      <path
        d={MARK_PATH}
        transform="translate(30.4 31) scale(1.2)"
        fill="none"
        stroke="var(--seal-ink)"
        strokeOpacity="0.82"
        strokeWidth="4.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
