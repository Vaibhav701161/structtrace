import { Link, Outlet, useNavigate } from "@tanstack/react-router";
import {
  Activity,
  BookOpen,
  Braces,
  Command,
  GitPullRequest,
  History,
  Home as HomeIcon,
  Menu,
  Moon,
  Plus,
  Settings,
  ShieldCheck,
  Sun,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Brand, Button, Status } from "../design-system/components";

const navigation = [
  { to: "/home", label: "Home", icon: HomeIcon },
  { to: "/runs", label: "Comparisons", icon: History },
  { to: "/regressions", label: "Regression cases", icon: Activity },
  { to: "/ci", label: "CI & integrations", icon: GitPullRequest },
] as const;

export function Shell() {
  const [collapsed, setCollapsed] = useState(false);
  const [palette, setPalette] = useState(false);
  const [dark, setDark] = useState(() => document.documentElement.dataset.theme === "dark");
  const navigate = useNavigate();

  useEffect(() => {
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
          <Link to="/settings/$section" params={{ section: "general" }} activeProps={{ className: "active" }}>
            <Settings size={18} aria-hidden="true" /><span>Settings</span>
          </Link>
        </nav>
        <div className="sidebar-bottom">
          <a href="https://github.com/Vaibhav701161/structtrace/tree/main/docs" target="_blank" rel="noreferrer"><BookOpen size={18} /><span>Documentation</span></a>
          <div className="local-status"><ShieldCheck size={16} /><span>Local only</span></div>
          <span className="version">StructTrace 0.1.0</span>
        </div>
      </aside>
      <section className="app-main">
        <header className="topbar">
          <button className="search-button" onClick={() => setPalette(true)}><Command size={16} /><span>Search or run a command</span><kbd>⌘ K</kbd></button>
          <div className="topbar-actions">
            <Status tone="pass" label="Local server" />
            <button className="icon-button" onClick={toggleTheme} aria-label="Toggle color theme">{dark ? <Sun size={18} /> : <Moon size={18} />}</button>
            <Button icon={Plus} onClick={() => void navigate({ to: "/new/source" })}>New comparison</Button>
          </div>
        </header>
        <main className="content" id="main-content"><Outlet /></main>
      </section>
      {palette && (
        <div className="modal-backdrop" role="presentation" onMouseDown={() => setPalette(false)}>
          <section className="command-palette" role="dialog" aria-modal="true" aria-label="Command palette" onMouseDown={(event) => event.stopPropagation()}>
            <div className="palette-search"><Command size={19} /><input autoFocus placeholder="Type a command…" aria-label="Search commands" /><button className="icon-button" onClick={() => setPalette(false)} aria-label="Close"><X size={18} /></button></div>
            <div className="palette-list">
              <button onClick={() => { setPalette(false); void navigate({ to: "/new/source" }); }}><Plus size={17} /><span><strong>New comparison</strong><small>Compare baseline and candidate outputs</small></span></button>
              <button onClick={() => { setPalette(false); void navigate({ to: "/runs" }); }}><History size={17} /><span><strong>Open comparisons</strong><small>Review immutable evidence</small></span></button>
              <button onClick={() => { setPalette(false); void navigate({ to: "/ci" }); }}><Braces size={17} /><span><strong>Generate CI</strong><small>Create a safe reproducible check</small></span></button>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
