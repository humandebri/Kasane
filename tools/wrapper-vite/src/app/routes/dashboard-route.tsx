// どこで: lazy route / 何を: dashboard route のナビゲーション連携を分離 / なぜ: ルーター本体から dashboard 依存を切り出して chunk 分割するため

import type { ReactElement } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { WrapperDashboard, type WrapperDashboardConfigState } from "@/components/dashboard";

export function DashboardRoute(
  {
    configState,
    view,
  }: {
    configState: WrapperDashboardConfigState;
    view: "console" | "history";
  },
): ReactElement {
  const navigate = useNavigate();
  const { requestId } = useParams<{ requestId: string }>();

  return (
    <WrapperDashboard
      {...configState}
      activeRequestId={requestId ?? null}
      statusModalOpen={requestId !== undefined}
      view={view}
      onOpenRequest={(nextRequestId) => navigate(view === "history" ? `/history/requests/${nextRequestId}` : `/requests/${nextRequestId}`)}
      onCloseRequest={() => navigate(view === "history" ? "/history" : "/")}
    />
  );
}
