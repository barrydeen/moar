import type { Metadata } from "next";
import { Inter } from "next/font/google";
import "./globals.css";
import { QueryProvider } from "@/lib/providers/query-provider";
import { Toaster } from "sonner";

const inter = Inter({ subsets: ["latin"] });

export const metadata: Metadata = {
  title: "MOAR Admin",
  description: "MOAR Relay Gateway Admin Panel",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" className="dark">
      <body className={inter.className}>
        <QueryProvider>
          {children}
          <Toaster theme="dark" />
        </QueryProvider>
      </body>
    </html>
  );
}
