// どこで: Verify導線ページ / 何を: APIベースの申請方法を案内 / なぜ: 認証付きsubmitをUIから辿れるようにするため

import { createFileRoute } from "@tanstack/react-router";
import { createServerFn } from "@tanstack/react-start";
import { validatePageInput } from "../../lib/page-input";
import { searchString } from "../../lib/url-search";

type Input = { address?: string };
const loadPage = createServerFn({ method: "GET" })
  .validator((input: Input) => validatePageInput(input, []))
  .handler(async ({ data: input }) => {
    const { loadConfig } = await import("../../lib/config");
    return { chainId: loadConfig(process.env).verifyDefaultChainId };
  });

export const Route = createFileRoute("/verify")({
  validateSearch: (search: Record<string, unknown>): Input => ({ address: searchString(search.address) }),
  loaderDeps: ({ search }) => search,
  loader: ({ deps }) => loadPage({ data: deps }),
  component: VerifyGuidePage,
});

function VerifyGuidePage() {
  const { chainId } = Route.useLoaderData();
  const { address } = Route.useSearch();
  const targetAddress = address?.trim() ?? "<contract_address>";
  return (
    <div className="space-y-4 rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
      <h1 className="text-xl font-semibold">Contract Verify</h1>
      <p className="text-sm text-slate-600">
        Verify申請は認証付きAPIで受け付けます。Bearer token を使って submit してください。
      </p>
      <pre className="overflow-x-auto rounded-md bg-slate-900 p-4 text-xs text-slate-100">
{`curl -X POST http://localhost:3000/api/verify/submit \\
  -H "content-type: application/json" \\
  -H "authorization: Bearer <YOUR_VERIFY_TOKEN>" \\
  -d '{
    "chainId": ${chainId},
    "contractAddress": "${targetAddress}",
    "compilerVersion": "0.8.30",
    "optimizerEnabled": true,
    "optimizerRuns": 200,
    "evmVersion": null,
    "sourceBundle": {
      "contracts/MyContract.sol": "pragma solidity ^0.8.30; contract MyContract {}"
    },
    "contractName": "MyContract",
    "constructorArgsHex": "0x"
  }'`}
      </pre>
      <p className="text-sm text-slate-600">
        進捗確認: <code className="font-mono">GET /api/verify/status?id=&lt;requestId&gt;</code>
      </p>
    </div>
  );
}
