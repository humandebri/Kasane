// どこで: Juno serverless functions / 何を: health と Recent Requests の query/update を公開 / なぜ: wrapper frontend の軽い運用導線と履歴永続化を担うため

import {
  defineAssert,
  defineQuery,
  defineUpdate,
  type AssertSetDoc,
} from "@junobuild/functions";
import { setDocStore, listDocsStore, encodeDocData, decodeDocData } from "@junobuild/functions/sdk";
import { caller } from "@junobuild/functions/ic-cdk";
import { z } from "zod";
import { probeHealth } from "@/lib/health";
import {
  createRecentRequestKey,
  RECENT_REQUESTS_COLLECTION,
  RECENT_REQUESTS_LIMIT,
  RecentRequestDocSchema,
  type RecentRequestDoc,
} from "@/lib/recent-requests";

const HealthFunctionResponseSchema = z.object({
  ok: z.boolean(),
  kasaneEvmReachable: z.boolean().optional(),
  wrapReachable: z.boolean().optional(),
  icHost: z.string().optional(),
  kasaneEvmCanisterId: z.string().optional(),
  wrapCanisterId: z.string().optional(),
  errorCode: z.string().optional(),
  message: z.string().optional(),
});

const RecentRequestResultSchema = z.object({
  principalText: z.string(),
  requestId: z.string(),
  kind: z.string(),
  submittedAt: z.string(),
});

const RecentRequestsResultSchema = z.object({
  entriesJson: z.string(),
});

function validateRecentRequestDoc(doc: RecentRequestDoc, callerPrincipalText: string): void {
  if (doc.principalText !== callerPrincipalText) {
    throw new Error("history.principal_mismatch");
  }
  if (createRecentRequestKey(doc.principalText, doc.requestId) === "") {
    throw new Error("history.key_invalid");
  }
}

export const health = defineQuery({
  result: HealthFunctionResponseSchema,
  handler: async () => probeHealth(),
});

const recentRequestsAssertDefinition: AssertSetDoc = {
  collections: [RECENT_REQUESTS_COLLECTION] as const,
  assert: ({ data }) => {
    const principalText = caller().toText();
    const decoded = RecentRequestDocSchema.parse(
      decodeDocData<RecentRequestDoc>(data.data.proposed.data),
    );
    const expectedKey = createRecentRequestKey(decoded.principalText, decoded.requestId);
    if (data.key !== expectedKey) {
      throw new Error("history.key_mismatch");
    }
    validateRecentRequestDoc(decoded, principalText);
  },
};

export const recent_requests_assert = defineAssert(recentRequestsAssertDefinition);

const RecentRequestArgsSchema = z.object({
  principalText: z.string(),
  requestId: z.string(),
  kind: z.string(),
  submittedAt: z.string(),
});

export const save_recent_request = defineUpdate({
  args: RecentRequestArgsSchema,
  result: RecentRequestResultSchema,
  handler: async (args) => {
    const principalText = caller().toText();
    const doc = RecentRequestDocSchema.parse(args);
    validateRecentRequestDoc(doc, principalText);
    setDocStore({
      caller: caller(),
      collection: RECENT_REQUESTS_COLLECTION,
      key: createRecentRequestKey(doc.principalText, doc.requestId),
      doc: {
        data: encodeDocData(doc),
      },
    });
    return doc;
  },
});

export const list_recent_requests = defineQuery({
  result: RecentRequestsResultSchema,
  handler: async () => {
    const owner = caller().toUint8Array();
    const docs = listDocsStore({
      caller: owner,
      collection: RECENT_REQUESTS_COLLECTION,
      params: {
        owner,
        order: {
          desc: true,
          field: "updated_at",
        },
        paginate: {
          limit: BigInt(RECENT_REQUESTS_LIMIT),
        },
      },
    });
    return {
      entriesJson: JSON.stringify(
        docs.items.map(([, doc]) =>
          RecentRequestDocSchema.parse(
            decodeDocData<RecentRequestDoc>(doc.data),
          ),
        ),
      ),
    };
  },
});
