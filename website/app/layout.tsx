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
  variable: "--font-sakura-display",
  display: "swap",
});

const body = Atkinson_Hyperlegible_Next({
  subsets: ["latin"],
  variable: "--font-sakura-body",
  adjustFontFallback: false,
  display: "swap",
});

const code = Atkinson_Hyperlegible_Mono({
  subsets: ["latin"],
  variable: "--font-sakura-code",
  adjustFontFallback: false,
  display: "swap",
});

export const metadata: Metadata = {
  metadataBase: new URL(site.url),
  title: { default: site.title, template: "%s · Sakura Lang" },
  description: site.description,
  applicationName: site.name,
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
