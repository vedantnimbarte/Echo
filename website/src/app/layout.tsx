import type { Metadata } from "next";
import { Open_Sans, JetBrains_Mono } from "next/font/google";
import "./globals.css";
import Nav from "@/components/site/Nav";
import Footer from "@/components/site/Footer";

const openSans = Open_Sans({
  variable: "--font-open-sans",
  subsets: ["latin"],
  display: "swap",
});

const jetbrains = JetBrains_Mono({
  variable: "--font-jetbrains",
  subsets: ["latin"],
  display: "swap",
});

export const metadata: Metadata = {
  title: "Echo — speak, it's typed.",
  description:
    "Echo turns your voice into text in any app — fully on-device, open-source, and private. Your voice never leaves your machine.",
  metadataBase: new URL("https://echo.app"),
  openGraph: {
    title: "Echo — speak, it's typed.",
    description:
      "On-device voice dictation for every app. Open-source. Private. Fast.",
    type: "website",
  },
};

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html
      lang="en"
      className={`${openSans.variable} ${jetbrains.variable}`}
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
