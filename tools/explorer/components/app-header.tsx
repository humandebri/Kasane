"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { Badge } from "./ui/badge";
import { Button, buttonVariants } from "./ui/button";
import { Input } from "./ui/input";
import { cn } from "../lib/utils";

const NAV_ITEMS = [
  { href: "/", label: "Home" },
  { href: "/ops", label: "Ops" },
  { href: "/logs", label: "Logs" },
];

export function AppHeader() {
  const pathname = usePathname();
  if (pathname === "/") {
    return null;
  }
  return (
    <header className="fade-in overflow-hidden rounded-2xl border border-slate-200 bg-white shadow-sm">
      <div className="flex flex-col gap-3 border-b border-slate-200 px-4 py-4 md:flex-row md:items-center md:justify-between">
        <div className="flex flex-wrap items-center gap-2">
          <Link href="/" className="text-xl font-semibold tracking-tight text-slate-900 hover:underline">
            Kasane Explorer
          </Link>
          <Badge className="border-transparent bg-sky-100 text-sky-800">testnet</Badge>
        </div>
        <nav className="flex flex-wrap gap-2">
          {NAV_ITEMS.map((item) => (
            <Link
              key={item.href}
              href={item.href}
              className={cn(buttonVariants({ variant: "secondary", size: "sm" }), "rounded-full bg-slate-100")}
            >
              {item.label}
            </Link>
          ))}
        </nav>
      </div>

      <div className="flex flex-col gap-2 bg-linear-to-r from-sky-50 via-white to-blue-50 px-4 py-4 md:flex-row md:items-center">
        <form action="/search" className="flex min-w-0 flex-1 gap-2">
          <Input
            name="q"
            required
            placeholder="Search: block / tx / address / principal"
            className="h-10 rounded-lg border-slate-300 bg-white font-mono"
          />
          <Button type="submit" className="rounded-full px-4">
            Search
          </Button>
        </form>
      </div>
    </header>
  );
}
