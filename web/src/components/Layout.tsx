import type { ReactNode } from "react";
import { Link, NavLink } from "react-router-dom";
import { cn } from "@/lib/utils";

const NAV = [
  { to: "/", label: "Search", end: true },
  { to: "/charts", label: "Charts", end: false },
  { to: "/api", label: "API", end: false },
  { to: "/donate", label: "Donate", end: false },
];

const navLink = ({ isActive }: { isActive: boolean }) =>
  cn(
    "text-primary-foreground no-underline transition-opacity hover:no-underline",
    isActive ? "font-bold opacity-100" : "opacity-85 hover:opacity-100",
  );

function Topbar() {
  return (
    <header className="sticky top-0 z-40 bg-primary text-primary-foreground">
      <div className="mx-auto flex h-13 max-w-[1400px] items-center justify-between gap-3 px-6 max-sm:px-4 max-[400px]:gap-2">
        <Link
          to="/"
          className="text-[17px] font-bold tracking-[-0.01em] text-primary-foreground no-underline hover:no-underline"
        >
          spotifree<span className="font-normal opacity-85">-rs</span>
        </Link>

        <nav className="flex items-center gap-[26px] text-[15px] max-sm:gap-4 max-sm:text-sm max-[400px]:gap-[11px] max-[400px]:text-[13px]">
          {NAV.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end}
              className={navLink}
            >
              {item.label}
            </NavLink>
          ))}
        </nav>
      </div>
    </header>
  );
}

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-svh flex-col">
      <Topbar />
      <div className="flex-1">{children}</div>
    </div>
  );
}
