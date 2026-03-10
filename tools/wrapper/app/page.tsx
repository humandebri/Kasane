// どこで: wrapper dashboard page / 何を: 最小ダッシュボードを表示 / なぜ: submitからstatus照会までを一画面で完結させるため

import {
  WrapperDashboard,
  type WrapperDashboardConfigState,
} from "@/components/dashboard";
import { loadConfig } from "@/lib/config";

function resolveConfig(): WrapperDashboardConfigState {
  try {
    return { cfg: loadConfig(), configError: null };
  } catch (error) {
    const message = error instanceof Error ? error.message : "config.invalid";
    return { cfg: null, configError: message };
  }
}

export default function Page() {
  const configState = resolveConfig();
  return <WrapperDashboard {...configState} />;
}
