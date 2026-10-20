import { useEffect, useState } from "react";
import { api } from "./api";
import type { WebApp } from "./types";
import { Dashboard } from "./components/Dashboard";
import { AppSidebar } from "./components/AppSidebar";
import { SettingsView } from "./components/SettingsView";
import { AppSettingsView } from "./components/AppSettingsView";
import { AddSubspacePopup } from "./components/AddSubspacePopup";
import { QuitConfirmPopup } from "./components/QuitConfirmPopup";
import { DownloadsPopup } from "./components/DownloadsPopup";
import { Toolbar } from "./components/Toolbar";
import { GlobalSettingsView } from "./components/GlobalSettingsView";
import { UnlockAppScreen } from "./components/UnlockAppScreen";
import "./App.css";

const params = new URLSearchParams(window.location.search);
const appId = params.get("appId");
const settingsApp = params.get("settingsApp");
const settingsSubspace = params.get("settingsSubspace");
const settingsForApp = params.get("settingsForApp");
const addPopupFor = params.get("addPopupFor");
const quitConfirmFor = params.get("quitConfirmFor");
const downloadsPopupFor = params.get("downloadsPopupFor");
const toolbarApp = params.get("toolbarApp");
const toolbarSubspace = params.get("toolbarSubspace");
const globalSettings = params.get("globalSettings");
const unlockApp = params.get("unlockApp");

function DashboardScreen() {
  const [apps, setApps] = useState<WebApp[]>([]);

  async function refresh() {
    setApps(await api.listApps());
  }

  useEffect(() => {
    refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, []);

  async function handleCreateApp(
    name: string,
    url: string,
    icon: string | null,
    fit: "cover" | "contain",
    rounded: boolean,
    background: string | null,
    paddingPercent: number,
  ) {
    await api.createApp(name, url, icon, fit, rounded, background, paddingPercent);
    await refresh();
  }

  async function handleDeleteApp(appIdToDelete: string) {
    await api.deleteApp(appIdToDelete);
    await refresh();
  }

  async function handleOpenApp(appIdToOpen: string, pin: string | null = null) {
    await api.openApp(appIdToOpen, pin);
  }

  return (
    <Dashboard
      apps={apps}
      onSelect={handleOpenApp}
      onCreate={handleCreateApp}
      onDelete={handleDeleteApp}
      onOpenGlobalSettings={() => api.openGlobalSettings()}
    />
  );
}

function App() {
  if (toolbarApp && toolbarSubspace) {
    return <Toolbar appId={toolbarApp} subspaceId={toolbarSubspace} />;
  }
  if (addPopupFor) {
    return <AddSubspacePopup appId={addPopupFor} />;
  }
  if (quitConfirmFor) {
    return <QuitConfirmPopup appId={quitConfirmFor} />;
  }
  if (downloadsPopupFor) {
    return <DownloadsPopup appId={downloadsPopupFor} />;
  }
  if (settingsApp && settingsSubspace) {
    return <SettingsView appId={settingsApp} subspaceId={settingsSubspace} />;
  }
  if (settingsForApp) {
    return <AppSettingsView appId={settingsForApp} />;
  }
  if (globalSettings) {
    return <GlobalSettingsView />;
  }
  if (unlockApp) {
    return <UnlockAppScreen appId={unlockApp} />;
  }
  if (appId) {
    return <AppSidebar appId={appId} />;
  }
  return <DashboardScreen />;
}

export default App;
