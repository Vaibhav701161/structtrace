import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Archive, ArrowRight, Copy, FolderOpen, History, Pin, ShieldCheck, Trash2 } from "lucide-react";
import { archiveProject, deletePinnedCase, duplicateProject, getPinnedCases, getProject, getProjects, getRuns, updatePinnedCase } from "../../api/client";
import { Button, Card, EmptyState, InlineNotice, PageHeader, Status } from "../../design-system/components";

import { useWorkspace } from "../../state/workspace";

export function SimplePage({ kind }: { kind: "runs" | "projects" | "regressions" | "settings" }) {
  if (kind === "runs") return <RunsPage />;
  if (kind === "projects") return <ProjectsPage />;
  if (kind === "regressions") return <RegressionsPage />;
  return <SettingsPage />;
}

function ProjectsPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { draft, loadProject, reset } = useWorkspace();
  const projects = useQuery({ queryKey: ["projects"], queryFn: getProjects });
  const open = useMutation({ mutationFn: getProject, onSuccess: (draft) => { loadProject(draft); void navigate({ to: "/new/source" }); } });
  const duplicate = useMutation({ mutationFn: duplicateProject, onSuccess: (draft) => { loadProject(draft); void queryClient.invalidateQueries({ queryKey: ["projects"] }); void navigate({ to: "/new/source" }); } });
  const archive = useMutation({ mutationFn: archiveProject, onSuccess: (_, projectId) => {
    queryClient.setQueryData<Awaited<ReturnType<typeof getProjects>>>(["projects"], (current) => current?.filter((project) => project.projectId !== projectId));
    if (draft.projectId === projectId) reset();
    void queryClient.invalidateQueries({ queryKey: ["projects"] });
    void queryClient.invalidateQueries({ queryKey: ["runs"] });
  } });
  const operationError = open.error ?? duplicate.error ?? archive.error;
  return <div className="page page-wide"><PageHeader title="Projects" description="Reopen, duplicate, or recoverably archive local projects. Every completed project points to a verified immutable revision." />{operationError && <InlineNotice tone="danger" title="Project operation failed">{operationError.message}</InlineNotice>}{projects.data?.length ? <Card className="pinned-list">{projects.data.map((project) => <div key={project.projectId}><span><FolderOpen size={17} /><span><strong>{project.name}</strong><small>{project.runCount} immutable {project.runCount === 1 ? "run" : "runs"} · {project.revisionId ? `revision ${project.revisionId}` : "legacy mutable project"}</small><small>{project.acceptedBaseline ? `Accepted from ${project.acceptedBaseline.runId} · ${project.acceptedBaseline.candidateArtifactHash.slice(0, 12)}…` : "No accepted baseline revision"}</small></span></span><div><Status tone={project.integrity.status === "verified" ? "pass" : "fail"} label={project.integrity.status === "verified" ? project.revisionState === "accepted" ? "Accepted revision" : "Verified revision" : "Integrity blocked"} /><Button variant="ghost" onClick={() => open.mutate(project.projectId)} disabled={open.isPending || project.integrity.status === "modified"}>Open</Button><button className="icon-button" aria-label={`Duplicate ${project.name}`} disabled={duplicate.isPending || project.integrity.status === "modified"} onClick={() => duplicate.mutate(project.projectId)}><Copy size={16} /></button><button className="icon-button" aria-label={`Archive ${project.name}`} disabled={archive.isPending && archive.variables === project.projectId} onClick={() => { if (window.confirm(`Archive ${project.name}? Evidence is moved to the local archived-projects directory, not deleted.`)) archive.mutate(project.projectId); }}><Archive size={16} /></button></div></div>)}</Card> : <Card><EmptyState icon={FolderOpen} title={projects.isLoading ? "Loading projects…" : "No saved projects"} description="Start a comparison; source references, mappings, rules, gate policy, and runs will stay under one project identity." /></Card>}</div>;
}

function RunsPage() {
  const navigate = useNavigate();
  const runs = useQuery({ queryKey: ["runs"], queryFn: getRuns });
  return <div className="page page-wide"><PageHeader title="Comparisons" description="Completed evidence is retained with an explicit integrity state and survives a local server restart." />{runs.data?.length ? <Card className="history-table"><div className="history-head"><span>Comparison</span><span>Decision</span><span>Deployment change</span><span>Independent cases</span><span /></div>{runs.data.map((run) => {
    const gate = run.gate;
    const difference = run.differencePp;
    const qualityFailed = (gate?.quality_failures.length ?? 0) > 0;
    const verified = run.integrity.status === "verified" && gate !== null && run.independentCases !== null;
    const label = !verified ? "Authority disabled" : gate.deployment_authorized ? "Release authorized" : qualityFailed ? "Do not deploy" : gate.status === "insufficient_evidence" ? "Not enough evidence" : gate.status === "failed" ? "Do not deploy" : gate.gate_mode === "regression" ? "Regression passed" : "Analysis complete";
    const tone = !verified || qualityFailed || gate?.status === "failed" ? "fail" : gate.status === "insufficient_evidence" ? "warning" : gate.deployment_authorized ? "pass" : "info";
    return <button className="history-row" key={run.runId} onClick={() => void navigate({ to: "/runs/$runId", params: { runId: run.runId } })}><span><strong>{run.projectName}</strong><small>{run.runId} · {verified ? "previously verified · cached receipt" : run.integrity.status}</small></span><Status tone={tone} label={verified ? "Previously verified" : label} /><code>{verified ? difference === null ? "not estimable" : `${difference >= 0 ? "+" : ""}${difference.toFixed(1)} pp` : "withheld"}</code><strong>{verified ? run.independentCases : "—"}</strong><ArrowRight size={16} /></button>;
  })}</Card> : <Card><EmptyState icon={History} title={runs.isLoading ? "Loading comparisons…" : "No production comparisons yet"} description="Complete a recorded-output comparison and its decision will appear here." /></Card>}</div>;
}

function RegressionsPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const pins = useQuery({ queryKey: ["regressions"], queryFn: getPinnedCases });
  const remove = useMutation({ mutationFn: deletePinnedCase, onSuccess: () => void queryClient.invalidateQueries({ queryKey: ["regressions"] }) });
  const update = useMutation({ mutationFn: ({ id, note, status }: { id: string; note: string; status: "open" | "fixed" }) => updatePinnedCase(id, note, status), onSuccess: () => void queryClient.invalidateQueries({ queryKey: ["regressions"] }) });
  return <div className="page page-wide">
    <PageHeader title="Saved cases" description="A recurring regression suite: pinned case IDs are reevaluated on every candidate run and enforced before release promotion and in generated CI." />
    {pins.data?.length ? <Card className="pinned-list saved-case-list">{pins.data.map((pin) => <div key={pin.id}>
      <span><Pin size={16} /><span><strong>{pin.caseId}</strong><small>{pin.projectName} · origin {pin.runId}{pin.lastRunId ? ` · last checked ${pin.lastRunId}` : ""}</small><input defaultValue={pin.note} placeholder="Add review note" aria-label={`Note for ${pin.caseId}`} onBlur={(event) => update.mutate({ id: pin.id, note: event.target.value, status: pin.status })} /></span></span>
      <div><Status tone={pin.suiteStatus === "passing" || pin.suiteStatus === "fixed" ? "pass" : "fail"} label={pin.suiteStatus.replaceAll("_", " ")} /><select value={pin.status} aria-label={`Review status for ${pin.caseId}`} onChange={(event) => update.mutate({ id: pin.id, note: pin.note, status: event.target.value as "open" | "fixed" })}><option value="open">Review open</option><option value="fixed">Reviewed</option></select><Button variant="ghost" onClick={() => void navigate({ to: "/runs/$runId/cases/$caseId", params: { runId: pin.lastRunId ?? pin.runId, caseId: pin.caseId }, search: { search: "", focus: "" } })}>Open latest evidence</Button><button className="icon-button" aria-label={`Remove ${pin.caseId}`} onClick={() => remove.mutate(pin.id)}><Trash2 size={16} /></button></div>
    </div>)}</Card> : <Card><EmptyState icon={Pin} title={pins.isLoading ? "Loading pinned suite…" : "No pinned cases"} description="Open a completed comparison and pin a case to enforce it on future candidates." /></Card>}
  </div>;
}

function SettingsPage() {
  return <div className="page"><PageHeader title="Local preferences" description="Only settings that persist from this screen are shown." /><Card className="settings-panel"><SettingRow title="Theme" text="Follow your system preference or keep a local browser override."><select defaultValue={window.localStorage.getItem("structtrace.theme") ?? "system"} onChange={(event) => applyTheme(event.target.value)}><option value="system">System</option><option value="light">Light</option><option value="dark">Dark</option></select></SettingRow><div className="settings-callout"><ShieldCheck /><div><strong>Runtime policy lives in structtrace.yaml</strong><p>Evidence, gates, retention, and limits are project configuration, not cosmetic UI preferences.</p></div><Status tone="info" label="Project-bound" /></div></Card></div>;
}
function applyTheme(theme: string) { if (theme === "system") { window.localStorage.removeItem("structtrace.theme"); document.documentElement.dataset.theme = window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"; } else { window.localStorage.setItem("structtrace.theme", theme); document.documentElement.dataset.theme = theme; } window.dispatchEvent(new Event("structtrace-theme")); }
function SettingRow({ title, text, children }: { title: string; text: string; children: React.ReactNode }) { return <label className="setting-row"><span><strong>{title}</strong><small>{text}</small></span>{children}</label>; }
