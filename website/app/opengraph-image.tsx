import { homeImageAlt, ink, ogImage, ogSize, quiet, sakuraInk } from "@/lib/og";
import { site } from "@/lib/site";

export const alt = homeImageAlt;
export const size = ogSize;
export const contentType = "image/png";

const code = ["fn signup", "(…) -> User ", "! {Clock, Db, Fail<SignupError>}"];

export default function OpengraphImage() {
  return ogImage(
    <div style={{ display: "flex", flexDirection: "column" }}>
      <div
        style={{
          fontFamily: "Display",
          fontSize: 84,
          lineHeight: 0.98,
          letterSpacing: "-0.04em",
          maxWidth: 1040,
        }}
      >
        {site.tagline}
      </div>
      <div
        style={{ display: "flex", marginTop: 40, fontFamily: "Mono", fontSize: 27, color: quiet }}
      >
        <span style={{ color: ink }}>{code[0]}</span>
        <span style={{ whiteSpace: "pre" }}>{code[1]}</span>
        <span style={{ color: sakuraInk }}>{code[2]}</span>
      </div>
    </div>,
    {
      footer: "Agent-first. Effects, errors and contracts, checked.",
      text: { Display: site.tagline, Mono: code.join("") },
    },
  );
}
