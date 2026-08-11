import { Link, Outlet, useNavigate } from "@tanstack/react-router";
import {
  Activity,
  BookOpen,
  Braces,
  Command,
  GitPullRequest,
  History,
  Home as HomeIcon,
  FolderOpen,
  Menu,
  Moon,
  Plus,
  ShieldCheck,
  Sun,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { getSystem } from "../api/client";
import { Brand, Button, Status } from "../design-system/components";
import { useWorkspace } from "../state/workspace";

const navigation = [
  { to: "/home", label: "Home", icon: HomeIcon },
  { to: "/runs", label: "Comparisons", icon: History },
  { to: "/projects", label: "Projects", icon: FolderOpen },
  { to: "/regressions", label: "Saved cases", icon: Activity },
  { to: "/ci", label: "CI export", icon: GitPullRequest },
] as const;
const commands = [
  { label: "New comparison", detail: "Compare baseline and candidate outputs", to: "/new/source", icon: Plus },
  { label: "Open comparisons", detail: "Review immutable evidence", to: "/runs", icon: History },
  { label: "Export CI project", detail: "Create a complete pinned integration snapshot", to: "/ci", icon: Braces },
] as const;

export function Shell() {
  const [collapsed, setCollapsed] = useState(false);
  const [palette, setPalette] = useState(false);
  const [mobileNav, setMobileNav] = useState(false);
  const [query, setQuery] = useState("");
  const [version, setVersion] = useState<string | null>(null);
  const [dark, setDark] = useState(() => document.documentElement.dataset.theme === "dark");
  const navigate = useNavigate();
  const { reset } = useWorkspace();

  useEffect(() => {
    void getSystem().then((system) => setVersion(system.version)).catch(() => setVersion(null));
    const listener = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault(); setPalette((value) => !value);
      }
    };
    const themeListener = () => setDark(document.documentElement.dataset.theme === "dark");
    window.addEventListener("keydown", listener);
    window.addEventListener("structtrace-theme", themeListener);
    return () => { window.removeEventListener("keydown", listener); window.removeEventListener("structtrace-theme", themeListener); };
  }, []);

  const toggleTheme = () => {
    const next = !dark;
    setDark(next);
    document.documentElement.dataset.theme = next ? "dark" : "light";
    window.localStorage.setItem("structtrace.theme", next ? "dark" : "light");
  };

  return (
    <div className={`app-shell ${collapsed ? "sidebar-collapsed" : ""}`}>
      <aside className="sidebar">
        <div className="sidebar-head">
          <Brand compact={collapsed} />
          <button className="icon-button sidebar-toggle" onClick={() => setCollapsed(!collapsed)} aria-label={collapsed ? "Expand navigation" : "Collapse navigation"}><Menu size={18} /></button>
        </div>
        <nav className="sidebar-nav" aria-label="Primary navigation">
          {navigation.map(({ to, label, icon: Icon }) => (
            <Link key={to} to={to} activeProps={{ className: "active" }} title={collapsed ? label : undefined}>
              <Icon size={18} aria-hidden="true" /><span>{label}</span>
            </Link>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <a href="https://github.com/Vaibhav701161/structtrace/tree/main/docs" target="_blank" rel="noreferrer"><BookOpen size={18} /><span>Documentation</span></a>
          <div className="local-status"><ShieldCheck size={16} /><span>Local only</span></div>
          <span className="version">{version ? `StructTrace ${version}` : "StructTrace"}</span>
        </div>
      </aside>
      <section className="app-main">
        <header className="topbar">
          <button className="icon-button mobile-menu" onClick={() => setMobileNav(true)} aria-label="Open navigation"><Menu size={20} /></button>
          <button className="search-button" onClick={() => setPalette(true)}><Command size={16} /><span>Search or run a command</span><kbd>⌘ K</kbd></button>
          <div className="topbar-actions">
            <Status tone="pass" label="Local server" />
            <button className="icon-button" onClick={toggleTheme} aria-label="Toggle color theme">{dark ? <Sun size={18} /> : <Moon size={18} />}</button>
            <Button icon={Plus} onClick={() => { reset(); void navigate({ to: "/new/source" }); }}>New comparison</Button>
          </div>
        </header>
        <main className="content" id="main-content"><Outlet /></main>
      </section>
      {mobileNav && <div className="mobile-nav-backdrop" onMouseDown={() => setMobileNav(false)}>
        <aside className="mobile-nav" role="dialog" aria-modal="true" aria-label="Mobile navigation" onMouseDown={(event) => event.stopPropagation()}>
          <div className="sidebar-head"><Brand /><button className="icon-button" onClick={() => setMobileNav(false)} aria-label="Close navigation"><X size={20} /></button></div>
          <nav className="sidebar-nav" aria-label="Primary navigation">
            {navigation.map(({ to, label, icon: Icon }) => <Link key={to} to={to} onClick={() => setMobileNav(false)}><Icon size={18} /><span>{label}</span></Link>)}
          </nav>
          <div className="local-status"><ShieldCheck size={16} /><span>Local only</span></div>
          <span className="version">{version ? `StructTrace ${version}` : "StructTrace"}</span>
        </aside>
      </div>}
      {palette && (
        <div className="modal-backdrop" role="presentation" onMouseDown={() => setPalette(false)}>
          <section className="command-palette" role="dialog" aria-modal="true" aria-label="Command palette" onMouseDown={(event) => event.stopPropagation()}>
            <div className="palette-search"><Command size={19} /><input autoFocus value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Type a command…" aria-label="Search commands" /><button className="icon-button" onClick={() => setPalette(false)} aria-label="Close"><X size={18} /></button></div>
            <div className="palette-list">
              {commands.filter((item) => `${item.label} ${item.detail}`.toLowerCase().includes(query.trim().toLowerCase())).map((item) => <button key={item.to} onClick={() => { if (item.to === "/new/source") reset(); setPalette(false); setQuery(""); void navigate({ to: item.to }); }}><item.icon size={17} /><span><strong>{item.label}</strong><small>{item.detail}</small></span></button>)}
              {!commands.some((item) => `${item.label} ${item.detail}`.toLowerCase().includes(query.trim().toLowerCase())) && query.trim() && <p className="palette-empty">No matching command</p>}
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
