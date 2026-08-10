import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { WorkspaceProvider } from "./state/workspace";
import { router } from "./app/router";
import "./styles/global.css";

const storedTheme = window.localStorage.getItem("structtrace.theme");
if (storedTheme === "dark" || (storedTheme !== "light" && window.matchMedia("(prefers-color-scheme: dark)").matches)) {
  document.documentElement.dataset.theme = "dark";
}

const queryClient = new QueryClient({ defaultOptions: { queries: { retry: 1, staleTime: 30_000 } } });

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <WorkspaceProvider>
        <RouterProvider router={router} />
      </WorkspaceProvider>
    </QueryClientProvider>
  </StrictMode>,
);
