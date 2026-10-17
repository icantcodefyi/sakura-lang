import { ImageResponse } from "next/og";
import { PETAL_ANGLES, SAKURA_CUTS } from "@/components/brand";
import { site } from "@/lib/site";

export const alt = "Sakura Lang: code that keeps its promises";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

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

export default async function OpengraphImage() {
  const host = new URL(site.url).host;
  const [display, mono] = await Promise.all([
    font("Funnel Display", 600, "Sakura Lang Code that keeps its promises."),
    font(
      "Atkinson Hyperlegible Mono",
      400,
      `fn signup(…) -> User ! {Clock, Db, Fail<SignupError>}The language agents write.${host}`,
    ),
  ]);
  const fonts = [
    display && { name: "Display", data: display, weight: 600 as const },
    mono && { name: "Mono", data: mono, weight: 400 as const },
  ].filter((f): f is NonNullable<typeof f> => Boolean(f));

  const ink = "#1e1e1e";
  const quiet = "#5f5f5f";
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
          Sakura Lang
        </span>
      </div>
      <div style={{ display: "flex", flexDirection: "column" }}>
        <div
          style={{
            fontFamily: "Display",
            fontSize: 104,
            lineHeight: 0.98,
            letterSpacing: "-0.04em",
            maxWidth: 900,
          }}
        >
          Code that keeps its promises.
        </div>
        <div
          style={{
            display: "flex",
            marginTop: 40,
            fontFamily: "Mono",
            fontSize: 27,
            color: quiet,
          }}
        >
          <span style={{ color: ink }}>fn signup</span>
          <span>(…) -&gt; User&nbsp;</span>
          <span style={{ color: "#b4245f" }}>! {"{Clock, Db, Fail<SignupError>}"}</span>
        </div>
      </div>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          fontFamily: "Mono",
          fontSize: 22,
          color: quiet,
        }}
      >
        <span>The language agents write.</span>
        <span>{host}</span>
      </div>
    </div>,
    { ...size, fonts: fonts.length ? fonts : undefined },
  );
}
