import type { ReactNode } from "react";
import { ImageResponse } from "next/og";
import { PETAL_ANGLES, SAKURA_CUTS } from "@/components/brand";
import { site } from "@/lib/site";

export const ogSize = { width: 1200, height: 630 };
export const homeImageAlt = `${site.name}: the programming language built for agents. Effects, errors and contracts, checked by the compiler.`;
export const ogHost = new URL(site.url).host;
export const ink = "#1e1e1e";
export const quiet = "#5f5f5f";
export const sakuraInk = "#b4245f";

type Family = "Display" | "Body" | "Mono";
const FAMILIES: Record<Family, [string, 400 | 600]> = {
  Display: ["Funnel Display", 600],
  Body: ["Atkinson Hyperlegible Next", 400],
  Mono: ["Atkinson Hyperlegible Mono", 400],
};

async function font(family: string, weight: number, text: string) {
  "use cache";
  // Fetch only the glyphs we draw (Google serves those as TrueType); fall back to the default font offline.
  try {
    const url = `https://fonts.googleapis.com/css2?family=${family.replace(/ /g, "+")}:wght@${weight}&text=${encodeURIComponent(text)}`;
    const css = await (await fetch(url)).text();
    const src = css.match(/src: url\((.+?)\) format\('(opentype|truetype)'\)/)?.[1];
    return src ? await (await fetch(src)).arrayBuffer() : null;
  } catch {
    return null;
  }
}

/** The share card: lockup on paper under the hero clouds, `children` in the middle, a footer line. */
export async function ogImage(
  children: ReactNode,
  { footer, text }: { footer: string; text: Partial<Record<Family, string>> },
) {
  const fonts = await Promise.all(
    (Object.keys(text) as Family[]).map(async (name) => {
      const [family, weight] = FAMILIES[name];
      const glyphs = `${text[name]}${name === "Display" ? site.name : ""}${name === "Mono" ? footer + ogHost : ""}`;
      const data = await font(family, weight, glyphs);
      return data && { name, data, weight };
    }),
  );
  const loaded = fonts.filter((f): f is NonNullable<typeof f> => Boolean(f));

  const cut = SAKURA_CUTS.main;
  return new ImageResponse(
    <div
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "column",
        justifyContent: "space-between",
        background: "#fefefe",
        color: ink,
        padding: "64px 80px 60px",
        // The hero clouds: sakura and magenta blooms at the corners, sky between, fading to paper.
        backgroundImage: [
          "radial-gradient(38% 60% at 0% 0%, rgba(243,134,161,0.95), rgba(243,134,161,0) 72%)",
          "radial-gradient(26% 46% at 24% -6%, rgba(212,91,182,0.6), rgba(212,91,182,0) 72%)",
          "radial-gradient(40% 58% at 58% -12%, rgba(159,195,231,0.95), rgba(159,195,231,0) 74%)",
          "radial-gradient(40% 70% at 104% 8%, rgba(243,134,161,0.95), rgba(243,134,161,0) 72%)",
          "radial-gradient(70% 70% at 50% 10%, rgba(252,231,238,1), rgba(252,231,238,0) 80%)",
        ].join(", "),
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 18 }}>
        <svg width="60" height="60" viewBox="0 0 100 100">
          <g transform={`translate(0 ${cut.dy})`}>
            {PETAL_ANGLES.map((a) => (
              <path key={a} d={cut.petal} fill="#f386a1" transform={`rotate(${a} 50 50)`} />
            ))}
            <circle cx="50" cy="50" r={cut.core} fill="#d45bb6" />
          </g>
        </svg>
        <span style={{ fontFamily: "Display", fontSize: 46, letterSpacing: "-0.025em" }}>
          {site.name}
        </span>
      </div>
      {children}
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          fontFamily: "Mono",
          fontSize: 22,
          color: quiet,
        }}
      >
        <span>{footer}</span>
        <span>{ogHost}</span>
      </div>
    </div>,
    { ...ogSize, fonts: loaded.length ? loaded : undefined },
  );
}
