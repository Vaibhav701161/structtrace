import {
  Outlet,
  createRootRoute,
  createRoute,
  createRouter,
  createBrowserHistory,
} from "@tanstack/react-router";
import { appBase } from "../api/client";
import { Shell } from "./shell";
import { Welcome } from "../features/home/Welcome";
import { Home } from "../features/home/Home";
import { Wizard } from "../features/wizard/Wizard";
import { Results } from "../features/results/Results";
import { Cases } from "../features/results/Cases";
import { Ci } from "../features/ci/Ci";
import { SimplePage } from "../features/misc/SimplePage";

const rootRoute = createRootRoute({ component: () => <Outlet /> });
const welcomeRoute = createRoute({ getParentRoute: () => rootRoute, path: "/", component: Welcome });
const appRoute = createRoute({ getParentRoute: () => rootRoute, id: "app", component: Shell });
const homeRoute = createRoute({ getParentRoute: () => appRoute, path: "/home", component: Home });
const runsRoute = createRoute({ getParentRoute: () => appRoute, path: "/runs", component: () => <SimplePage kind="runs" /> });
const regressionsRoute = createRoute({ getParentRoute: () => appRoute, path: "/regressions", component: () => <SimplePage kind="regressions" /> });
const ciRoute = createRoute({ getParentRoute: () => appRoute, path: "/ci", component: Ci });
const settingsRoute = createRoute({ getParentRoute: () => appRoute, path: "/settings/$section", component: () => <SimplePage kind="settings" /> });
const resultRoute = createRoute({ getParentRoute: () => appRoute, path: "/runs/$runId", component: Results });
const casesRoute = createRoute({ getParentRoute: () => appRoute, path: "/runs/$runId/cases", component: Cases });
const caseRoute = createRoute({ getParentRoute: () => appRoute, path: "/runs/$runId/cases/$caseId", component: Cases });
const wizardRoutes = [
  ["/new/source", 0], ["/new/map", 1], ["/new/correctness", 2], ["/new/evidence", 3], ["/new/review", 4], ["/new/run", 5],
] as const;
const wizard = wizardRoutes.map(([path, step]) => createRoute({
  getParentRoute: () => appRoute,
  path,
  component: () => <Wizard step={step} />,
}));

const routeTree = rootRoute.addChildren([
  welcomeRoute,
  appRoute.addChildren([homeRoute, runsRoute, regressionsRoute, ciRoute, settingsRoute, resultRoute, casesRoute, caseRoute, ...wizard]),
]);

export const router = createRouter({
  routeTree,
  history: createBrowserHistory(),
  basepath: appBase || "/",
  defaultPreload: "intent",
});

declare module "@tanstack/react-router" {
  interface Register { router: typeof router }
}
