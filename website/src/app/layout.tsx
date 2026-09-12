import type { Metadata } from "next";
import { EB_Garamond, Figtree } from "next/font/google";
import "./globals.css";
import Nav from "@/components/site/Nav";
import Footer from "@/components/site/Footer";

/* The same two families the app is set in, divided the same way: a book face
   for headings, a grotesque for everything that involves operating rather than
   reading. Garamond is latin-only and pulled at the single weight it is used
   at — the app ships exactly this cut. */
const garamond = EB_Garamond({
  variable: "--font-garamond",
  subsets: ["latin"],
  weight: ["500"],
  display: "swap",
});

const figtree = Figtree({
  variable: "--font-figtree",
  subsets: ["latin"],
  display: "swap",
});

export const metadata: Metadata = {
  title: "Echo — say the word, start talking",
  description:
    "Echo is a voice keyboard for macOS, Windows, and Linux. Say your wake word and it types what you say into whatever app is focused — wake word, transcription, and models all running on your own machine.",
  metadataBase: new URL("https://echo.app"),
  openGraph: {
    title: "Echo — say the word, start talking",
    description:
      "Hands-free dictation into any app. On-device, open-source, MIT.",
    type: "website",
  },
};

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html
      lang="en"
      className={`${garamond.variable} ${figtree.variable}`}
    >
      <body className="grain min-h-screen antialiased">
        <div className="field" aria-hidden />
        <Nav />
        <main>{children}</main>
        <Footer />
      </body>
    </html>
  );
}
