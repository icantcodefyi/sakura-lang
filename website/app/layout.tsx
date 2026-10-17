import type { Metadata, Viewport } from "next";
import {
  Atkinson_Hyperlegible_Mono,
  Atkinson_Hyperlegible_Next,
  Funnel_Display,
} from "next/font/google";
import { RootProvider } from "fumadocs-ui/provider/next";
import { site } from "@/lib/site";
import "./globals.css";

const display = Funnel_Display({
  subsets: ["latin"],
  // Every display setting is 600; one static instance is half the variable font's bytes.
  weight: "600",
  variable: "--font-sakura-display",
  display: "swap",
});

const body = Atkinson_Hyperlegible_Next({
  subsets: ["latin"],
  variable: "--font-sakura-body",
  // next/font has no metrics for Atkinson; globals.css defines these metric-matched fallbacks.
  fallback: ["Sakura Body Fallback"],
  adjustFontFallback: false,
  display: "swap",
});

const code = Atkinson_Hyperlegible_Mono({
  subsets: ["latin"],
  variable: "--font-sakura-code",
  fallback: ["Sakura Code Fallback"],
  adjustFontFallback: false,
  display: "swap",
});

export const metadata: Metadata = {
  metadataBase: new URL(site.url),
  title: { default: site.title, template: "%s · Sakura Lang" },
  description: site.description,
  applicationName: site.name,
  authors: [{ name: site.author.name, url: site.author.github }],
  creator: site.author.name,
  publisher: site.author.name,
  category: "technology",
  keywords: [
    "Sakura Lang",
    "programming language",
    "AI agents",
    "effects",
    "refinement types",
    "contracts",
    "llms.txt",
    "MCP",
  ],
  // Pages without their own (the 404 page) still get a share card; pages set canonical themselves.
  openGraph: { type: "website", siteName: site.name, locale: "en_US" },
  twitter: { card: "summary_large_image", site: site.author.handle, creator: site.author.handle },
};

export const viewport: Viewport = {
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#fefefe" },
    { media: "(prefers-color-scheme: dark)", color: "#1e1e1e" },
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
