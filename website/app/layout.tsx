import type { Metadata, Viewport } from "next";
import { Atkinson_Hyperlegible_Mono, Atkinson_Hyperlegible_Next, Funnel_Display } from "next/font/google";
import { RootProvider } from "fumadocs-ui/provider/next";
import { site } from "@/lib/site";
import "./globals.css";

const display = Funnel_Display({
  subsets: ["latin"],
  variable: "--font-vow-display",
  display: "swap",
});

const body = Atkinson_Hyperlegible_Next({
  subsets: ["latin"],
  variable: "--font-vow-body",
  adjustFontFallback: false,
  display: "swap",
});

const code = Atkinson_Hyperlegible_Mono({
  subsets: ["latin"],
  variable: "--font-vow-code",
  adjustFontFallback: false,
  display: "swap",
});

export const metadata: Metadata = {
  metadataBase: new URL(site.url),
  title: { default: site.title, template: "%s · Vow" },
  description: site.description,
  applicationName: site.name,
  keywords: ["Vow", "programming language", "AI agents", "effects", "refinement types", "contracts", "llms.txt", "MCP"],
  openGraph: {
    type: "website",
    siteName: site.name,
    title: site.title,
    description: site.description,
    url: "/",
  },
  twitter: { card: "summary_large_image", title: site.title, description: site.description },
  alternates: {
    canonical: "/",
    types: { "text/plain": [{ url: "/llms.txt", title: "llms.txt" }] },
  },
};

export const viewport: Viewport = {
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#f9fafb" },
    { media: "(prefers-color-scheme: dark)", color: "#16181c" },
  ],
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html
      lang="en"
      suppressHydrationWarning
      className={`${display.variable} ${body.variable} ${code.variable}`}
    >
      <body className="flex min-h-dvh flex-col font-sans">
        <RootProvider>{children}</RootProvider>
      </body>
    </html>
  );
}
