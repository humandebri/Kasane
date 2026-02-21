"use client";

import { cilCheckCircle } from "@coreui/icons";
import CIcon from "@coreui/icons-react";

export function StatusSuccessIcon({ className }: { className?: string }) {
  return <CIcon icon={cilCheckCircle} className={className} />;
}
