import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { PillApp } from "./windows/PillApp";
import "./styles.css";
import { setLocale } from "./i18n";
import { commands } from "./ipc/commands";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, staleTime: 5000 },
  },
});

// One bundle serves both webviews; pick the root by window label.
const isPill = getCurrentWindow().label === "pill";

function mount() {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        {isPill ? <PillApp /> : <App />}
      </QueryClientProvider>
    </React.StrictMode>
  );
}

// Resolve the interface language before the first render, so nothing flashes
// in English and then swaps. Both webviews do this independently — they are
// separate JS contexts and neither can read the other's state.
//
// A failure here must not stop the app from starting: English is a working
// fallback, an unmounted window is not.
commands
  .getSetting("ui_language")
  .then((stored) => setLocale(stored))
  .catch(() => setLocale(null))
  .finally(mount);
