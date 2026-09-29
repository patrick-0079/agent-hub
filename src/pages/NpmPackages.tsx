/** npm 全局包：清单与 MCP 关联标记（声明式安装 M3）。 */

import React, { useMemo, useState } from "react";
import { Icon } from "../components/Icon";
import {
  Badge,
  Card,
  Empty,
  SearchInput,
  SectionCard,
  SegmentedControl,
  Toggle,
} from "../components/ui";
import { shortenPath } from "../lib/format";
import { useApp } from "../lib/store";

export function NpmPage() {
  const snapshot = useApp((s) => s.snapshot);
  const [query, setQuery] = useState("");
  const [manager, setManager] = useState("all");
  const [onlyMcp, setOnlyMcp] = useState(false);

  const packages = snapshot?.npmPackages ?? [];

  const counts = useMemo(
    () => ({
      npm: packages.filter((p) => p.manager === "npm").length,
      pnpm: packages.filter((p) => p.manager === "pnpm").length,
    }),
    [packages],
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return packages.filter((p) => {
      if (manager !== "all" && p.manager !== manager) return false;
      if (onlyMcp && !p.mcpCapable) return false;
      if (!q) return true;
      return p.name.toLowerCase().includes(q) || p.version.toLowerCase().includes(q);
    });
  }, [packages, query, manager, onlyMcp]);

  return (
    <div className="space-y-4">
      <SectionCard
        title="npm 全局包"
        subtitle={`共 ${packages.length} 个（npm ${counts.npm} · pnpm ${counts.pnpm}）`}
        action={
          <div className="w-56">
            <SearchInput value={query} onChange={setQuery} placeholder="搜索包名 / 版本…" />
          </div>
        }
        bodyClassName="space-y-3"
      >
        <div className="flex flex-wrap items-center gap-3">
          <SegmentedControl
            value={manager}
            onChange={setManager}
            options={[
              { value: "all", label: "全部", count: packages.length },
              { value: "npm", label: "npm", count: counts.npm },
              { value: "pnpm", label: "pnpm", count: counts.pnpm },
            ]}
          />
          <div className="w-56 rounded-lg border border-ink-800 bg-ink-900 px-3">
            <Toggle
              checked={onlyMcp}
              onChange={setOnlyMcp}
              label="只看与 MCP 相关"
              hint="包名包含 mcp 或 @modelcontextprotocol"
            />
          </div>
        </div>

        {filtered.length === 0 ? (
          <Empty
            icon="npm"
            title={packages.length === 0 ? "未读取到全局包" : "没有匹配的包"}
            description={
              packages.length === 0
                ? "AgentHub 通过 `npm ls -g --json` 与 `pnpm ls -g --json` 读取全局清单。若命令不可用或超时，可在设置里指定 npm / pnpm 路径后重试。"
                : "调整筛选条件试试。"
            }
          />
        ) : (
          <div className="overflow-hidden rounded-lg border border-ink-800/70">
            <table className="w-full border-collapse">
              <thead>
                <tr>
                  <th className="table-head px-3 py-2">包名</th>
                  <th className="table-head w-28 px-3 py-2">版本</th>
                  <th className="table-head w-24 px-3 py-2">管理器</th>
                  <th className="table-head w-32 px-3 py-2">标记</th>
                  <th className="table-head px-3 py-2">安装位置</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((pkg) => (
                  <tr key={`${pkg.manager}-${pkg.name}`} className="hover:bg-ink-800">
                    <td className="table-cell px-3">
                      <div className="flex items-center gap-2">
                        <Icon name="npm" className="h-3.5 w-3.5 shrink-0 text-amber-300" />
                        <span className="truncate font-mono text-xs text-slate-200">{pkg.name}</span>
                      </div>
                    </td>
                    <td className="table-cell px-3 font-mono text-[11px] text-brand-400">
                      {pkg.version}
                    </td>
                    <td className="table-cell px-3">
                      <Badge tone={pkg.manager === "pnpm" ? "violet" : "slate"}>{pkg.manager}</Badge>
                    </td>
                    <td className="table-cell px-3">
                      {pkg.mcpCapable ? (
                        <Badge tone="sky" icon="mcp">
                          MCP
                        </Badge>
                      ) : (
                        <span className="text-slate-600">—</span>
                      )}
                    </td>
                    <td className="table-cell px-3">
                      <span className="mono truncate" title={pkg.location ?? ""}>
                        {pkg.location ? shortenPath(pkg.location, 46) : "—"}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </SectionCard>

      <div className="grid gap-3 md:grid-cols-3">
        <Card className="border-dashed">
          <div className="flex items-center gap-2">
            <Icon name="plus" className="h-4 w-4 text-accent-400" />
            <span className="text-sm text-slate-200">声明式安装 / 卸载</span>
            <Badge tone="violet" className="ml-auto">
              M3
            </Badge>
          </div>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
            在界面里声明需要的全局工具与版本约束，由 AgentHub 执行安装并流式展示过程输出。
          </p>
        </Card>
        <Card className="border-dashed">
          <div className="flex items-center gap-2">
            <Icon name="refresh" className="h-4 w-4 text-accent-400" />
            <span className="text-sm text-slate-200">版本锁定与更新提示</span>
            <Badge tone="violet" className="ml-auto">
              M3
            </Badge>
          </div>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
            记录每个 Profile 期望的版本，检测可更新项并展示升级影响。
          </p>
        </Card>
        <Card className="border-dashed">
          <div className="flex items-center gap-2">
            <Icon name="mcp" className="h-4 w-4 text-accent-400" />
            <span className="text-sm text-slate-200">与 MCP 联动</span>
            <Badge tone="violet" className="ml-auto">
              M3
            </Badge>
          </div>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
            npx / uvx 型 MCP 的依赖包缺失时，直接在内一键安装后再做握手检查。
          </p>
        </Card>
      </div>
    </div>
  );
}