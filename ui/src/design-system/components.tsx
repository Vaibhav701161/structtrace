import {
  AlertCircle,
  ArrowRight,
  Check,
  CheckCircle2,
  ChevronRight,
  Circle,
  Info,
  LoaderCircle,
  TriangleAlert,
  XCircle,
  type LucideIcon,
} from "lucide-react";
import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from "react";
import { appBase } from "../api/client";

export function Brand({ compact = false }: { compact?: boolean }) {
  return (
    <span className="brand" aria-label="StructTrace">
      <img src={`${appBase}/assets/structtrace-logo-mark.svg`} alt="" width="38" height="38" />
      {!compact && <span>StructTrace</span>}
    </span>
  );
}

export function Button({
  variant = "primary",
  icon: Icon,
  children,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  icon?: LucideIcon;
}) {
  return (
    <button className={`button button-${variant}`} {...props}>
      {Icon && <Icon size={17} aria-hidden="true" />}
      <span>{children}</span>
    </button>
  );
}

export function Status({
  tone,
  label,
  detail,
}: {
  tone: "pass" | "fail" | "warning" | "info" | "neutral" | "working";
  label: string;
  detail?: string;
}) {
  const icons = { pass: CheckCircle2, fail: XCircle, warning: TriangleAlert, info: Info, neutral: Circle, working: LoaderCircle };
  const Icon = icons[tone];
  return (
    <span className={`status status-${tone}`} title={detail}>
      <Icon size={15} aria-hidden="true" className={tone === "working" ? "spin" : undefined} />
      <span>{label}</span>
    </span>
  );
}

export function InlineNotice({
  tone = "info",
  title,
  children,
}: {
  tone?: "info" | "warning" | "danger" | "success";
  title: string;
  children: ReactNode;
}) {
  const Icon = tone === "danger" ? AlertCircle : tone === "warning" ? TriangleAlert : tone === "success" ? CheckCircle2 : Info;
  return (
    <section className={`notice notice-${tone}`} aria-label={title}>
      <Icon size={18} aria-hidden="true" />
      <div><strong>{title}</strong><div>{children}</div></div>
    </section>
  );
}

export function PageHeader({ eyebrow, title, description, actions }: {
  eyebrow?: string;
  title: string;
  description?: string;
  actions?: ReactNode;
}) {
  return (
    <header className="page-header">
      <div>
        {eyebrow && <div className="eyebrow">{eyebrow}</div>}
        <h1>{title}</h1>
        {description && <p>{description}</p>}
      </div>
      {actions && <div className="page-actions">{actions}</div>}
    </header>
  );
}

export function Card({ children, className = "", ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={`card ${className}`} {...props}>{children}</div>;
}

export function EmptyState({ icon: Icon, title, description, action }: {
  icon: LucideIcon;
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <div className="empty-icon"><Icon size={22} aria-hidden="true" /></div>
      <h2>{title}</h2>
      <p>{description}</p>
      {action}
    </div>
  );
}

export const steps = ["Sources", "Map fields", "Define correctness", "Evidence & decision", "Review", "Run"];

export function Stepper({ current }: { current: number }) {
  return (
    <nav className="stepper" aria-label="Comparison setup">
      {steps.map((step, index) => {
        const complete = index < current;
        return (
          <div className={`step ${index === current ? "current" : ""} ${complete ? "complete" : ""}`} key={step}>
            <span className="step-index" aria-hidden="true">{complete ? <Check size={14} /> : index + 1}</span>
            <span>{step}</span>
            {index < steps.length - 1 && <ChevronRight size={14} className="step-arrow" aria-hidden="true" />}
          </div>
        );
      })}
    </nav>
  );
}

export function WizardActions({ back, next, nextLabel = "Continue", disabled = false }: {
  back?: () => void;
  next: () => void;
  nextLabel?: string;
  disabled?: boolean;
}) {
  return (
    <div className="wizard-actions">
      <div>{back && <Button variant="ghost" onClick={back}>Back</Button>}</div>
      <Button onClick={next} disabled={disabled} icon={ArrowRight}>{nextLabel}</Button>
    </div>
  );
}

export function Skeleton({ width = "100%" }: { width?: string }) {
  return <span className="skeleton" style={{ width }} aria-hidden="true" />;
}
