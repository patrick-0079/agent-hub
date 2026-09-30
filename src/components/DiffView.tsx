/** 写入前的 diff 视图：按行着色（+ 新增 / - 删除 / @@ 折叠提示）。 */

import React from "react";
import { Icon } from "./Icon";

export function DiffView({
  diff,
  emptyHint = "无变更",
  maxHeight = "max-h-[420px]",
}: {
  diff: string;
  emptyHint?: string;
  maxHeight?: string;
}) {
  if (!diff.trim()) {
    return (
      <div className="flex items-center gap-2 rounded-lg border border-ink-800 bg-ink-950 px-3 py-4 text-xs text-slate-500">
        <Icon name="check" className="h-3.5 w-3.5 text-brand-400" />
        {emptyHint}
      </div>
    );
  }

  const lines = diff.replace(/\n$/, "").split("\n");
  return (
    <div className={`${maxHeight} overflow-auto rounded-lg border border-ink-800 bg-ink-950`}>
      <pre className="min-w-full p-0 text-[11.5px] leading-[1.55]">
        {lines.map((line, index) => {
          const kind = line.startsWith("+")
            ? "add"
            : line.startsWith("-")
              ? "del"
              : line.startsWith("@@")
                ? "meta"
                : "ctx";
          const cls =
            kind === "add"
              ? "bg-brand-900 text-brand-300"
              : kind === "del"
                ? "bg-rose-950 text-rose-300"
                : kind === "meta"
                  ? "bg-ink-800 text-slate-500"
                  : "text-slate-400";
          return (
            <div key={index} className={`flex ${cls}`}>
              <span className="w-10 shrink-0 select-none border-r border-ink-800 px-2 text-right font-mono text-[10px] text-slate-600">
                {index + 1}
              </span>
              <span className="min-w-0 flex-1 whitespace-pre-wrap break-all px-3">
                {line || " "}
              </span>
            </div>
          );
        })}
      </pre>
    </div>
  );
}