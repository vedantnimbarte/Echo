import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { PillApp } from "./windows/PillApp";
import "./styles.css";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, staleTime: 5000 },
  },
});

// One bundle serves both webviews; pick the root by window label.
const isPill = getCurrentWindow().label === "pill";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      {isPill ? <PillApp /> : <App />}
    </QueryClientProvider>
  </React.StrictMode>
);
