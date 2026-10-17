import { ImageResponse } from "next/og";
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

  const ink = "#16181c";
  const seal = "#f2b53f";
  return new ImageResponse(
    <div
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "column",
        justifyContent: "space-between",
        background: ink,
        color: "#eef0f3",
        padding: "72px 80px",
        backgroundImage:
          "repeating-linear-gradient(to bottom, transparent 0 47px, #1e2126 47px 48px)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 18 }}>
        <svg width="56" height="56" viewBox="0 0 32 32">
          <path
            d="M7.5 15.5 13.6 24 25 7.5"
            stroke={seal}
            strokeWidth="5"
            strokeLinecap="round"
            strokeLinejoin="round"
            fill="none"
          />
        </svg>
        <span style={{ fontFamily: "Display", fontSize: 44, letterSpacing: "-0.03em" }}>
          Sakura Lang
        </span>
      </div>
      <div style={{ display: "flex", flexDirection: "column" }}>
        <div
          style={{ fontFamily: "Display", fontSize: 92, lineHeight: 1, letterSpacing: "-0.035em" }}
        >
          Code that keeps its promises.
        </div>
        <div
          style={{
            display: "flex",
            marginTop: 40,
            fontFamily: "Mono",
            fontSize: 27,
            color: "#9aa8bc",
          }}
        >
          <span style={{ color: "#f5f6f8" }}>fn signup</span>
          <span>(…) -&gt; User&nbsp;</span>
          <span style={{ color: seal }}>! {"{Clock, Db, Fail<SignupError>}"}</span>
        </div>
      </div>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          fontFamily: "Mono",
          fontSize: 22,
          color: "#8a919c",
        }}
      >
        <span>The language agents write.</span>
        <span>{host}</span>
      </div>
    </div>,
    { ...size, fonts: fonts.length ? fonts : undefined },
  );
}
