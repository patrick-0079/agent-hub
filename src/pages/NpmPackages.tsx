/** npm 全局包：清单与 MCP 关联标记 + 声明式安装/卸载（T3 计划确认）。 */

import React, { useMemo, useState } from "react";
import { Icon } from "../components/Icon";
import {
  Badge,
  Card,
  Empty,
  Modal,
  SearchInput,
  SectionCard,
  SegmentedControl,
  StatusDot,
  Toggle,
} from "../components/ui";
import { api, describeError } from "../lib/api";
import { shortenPath } from "../lib/format";
import { useApp } from "../lib/store";
import type { ActionResult, NpmInstallPlan, NpmOutdated, NpmPackage } from "../lib/types";

/* ------------------------------------------------------------ 安装弹窗 */

function InstallDialog({
  open,
  onClose,
  onDone,
}: {
  open: boolean;
  onClose: () => void;
  onDone: () => void;
}) {
  const setBanner = useApp((s) => s.setBanner);
  const [manager, setManager] = useState("npm");
  const [packagesText, setPackagesText] = useState("");
  const [plan, setPlan] = useState<NpmInstallPlan | null>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ActionResult | null>(null);

  const packages = packagesText
    .split(/[\n,，\s]+/)
    .map((p) => p.trim())
    .filter(Boolean);

  const refreshPlan = (nextManager: string, nextPackages: string[]) => {
    if (nextPackages.length === 0) {
      setPlan(null);
      return;
    }
    api
      .npmInstallPlan(nextManager, nextPackages)
      .then(setPlan)
      .catch((error) => setBanner(describeError(error)));
  };

  const install = async () => {
    setBusy(true);
    setResult(null);
    try {
      const r = await api.npmInstallRun(manager, packages);
      setResult(r);
      if (r.ok) onDone();
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const blocked = busy || packages.length === 0 || (plan != null && !plan.found);

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="全局安装 npm 包"
      subtitle="T3 变更操作：真实执行包管理器；先看计划再确认"
      width="max-w-2xl"
      footer={
        result?.ok ? (
          <button type="button" className="btn-primary" onClick={onClose}>
            完成
          </button>
        ) : (
          <>
            <button type="button" className="btn-ghost" onClick={onClose} disabled={busy}>
              取消
            </button>
            <button type="button" className="btn-primary" onClick={() => void install()} disabled={blocked}>
              {busy ? "安装中…（可能要等网络）" : `安装 ${packages.length} 个包`}
            </button>
          </>
        )
      }
    >
      <div className="space-y-3.5">
        <div className="grid gap-3 sm:grid-cols-[130px_1fr]">
          <div>
            <label className="text-xs text-slate-400">管理器</label>
            <div className="mt-1.5">
              <SegmentedControl
                value={manager}
                onChange={(next) => {
                  setManager(next);
                  refreshPlan(next, packages);
                }}
                options={[
                  { value: "npm", label: "npm" },
                  { value: "pnpm", label: "pnpm" },
                ]}
              />
            </div>
          </div>
          <div>
            <label className="text-xs text-slate-400">包名（一行一个，支持空格/逗号分隔）</label>
            <textarea
              value={packagesText}
              onChange={(e) => {
                setPackagesText(e.target.value);
                refreshPlan(manager, packages);
              }}
              placeholder={"@upstash/context7-mcp\nmcp-server-time"}
              spellCheck={false}
              className="input mt-1.5 h-24 font-mono text-xs"
            />
          </div>
        </div>

        {plan && (
          <div className="rounded-md border border-ink-800 bg-ink-900 p-3">
            <div className="flex items-center gap-2">
              <StatusDot state={plan.found ? "ok" : "error"} />
              <span className="text-xs font-medium text-slate-200">将执行（{plan.tierCode}）</span>
            </div>
            <pre className="mono mt-1.5 whitespace-pre-wrap break-all text-[10.5px] text-slate-400">
              {plan.managerPath} {plan.args.join(" ")}
            </pre>
            <p className="mt-1.5 text-[11px] text-slate-500">{plan.message}</p>
          </div>
        )}

        {result && (
          <div
            className={`rounded-md border p-3 ${
              result.ok ? "border-brand-800 bg-brand-900" : "border-rose-800/70 bg-rose-950"
            }`}
          >
            <div className="flex items-center gap-2 text-xs">
              <StatusDot state={result.ok ? "ok" : "error"} />
              <span className={result.ok ? "text-brand-400" : "text-rose-300"}>
                {result.summary || result.title}
              </span>
            </div>
            {result.steps.map((step, i) => (
              <div key={i} className="mono mt-1.5 whitespace-pre-wrap break-all text-[10.5px]">
                <span className={step.ok ? "text-slate-400" : "text-rose-300"}>
                  {step.ok ? "✓" : "✗"} {step.target}
                </span>
                <span className="text-slate-500"> — {step.message}</span>
              </div>
            ))}
          </div>
        )}

        <p className="text-[11px] leading-relaxed text-slate-500">
          安装后重新扫描即可在列表看到新版本；版本号由 <span className="font-mono">ls -g</span> 读回并落库为受管记录。
        </p>
      </div>
    </Modal>
  );
}

/* ------------------------------------------------------------ 卸载确认 */

function RemoveDialog({
  pkg,
  onClose,
  onDone,
}: {
  pkg: NpmPackage | null;
  onClose: () => void;
  onDone: () => void;
}) {
  const setBanner = useApp((s) => s.setBanner);
  const [plan, setPlan] = useState<NpmInstallPlan | null>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ActionResult | null>(null);

  React.useEffect(() => {
    if (pkg) {
      setPlan(null);
      setResult(null);
      api
        .npmRemovePlan(pkg.manager, pkg.name)
        .then(setPlan)
        .catch((error) => setBanner(describeError(error)));
    }
  }, [pkg, setBanner]);

  const remove = async () => {
    if (!pkg) return;
    setBusy(true);
    try {
      const r = await api.npmRemoveRun(pkg.manager, pkg.name);
      setResult(r);
      if (r.ok) onDone();
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={pkg != null}
      onClose={onClose}
      title={`卸载全局包：${pkg?.name ?? ""}`}
      subtitle="T3 变更操作：走包管理器 uninstall（可随时重装），受管记录一并清除"
      width="max-w-xl"
      footer={
        result?.ok ? (
          <button type="button" className="btn-primary" onClick={onClose}>
            完成
          </button>
        ) : (
          <>
            <button type="button" className="btn-ghost" onClick={onClose} disabled={busy}>
              取消
            </button>
            <button
              type="button"
              className="btn border border-rose-500/40 text-rose-300 hover:bg-rose-950"
              onClick={() => void remove()}
              disabled={busy || (plan != null && !plan.found)}
            >
              {busy ? "卸载中…" : "确认卸载"}
            </button>
          </>
        )
      }
    >
      <div className="space-y-3">
        {pkg && (
          <div className="rounded-md border border-ink-800 bg-ink-900 p-3 text-xs">
            <div className="flex flex-wrap items-center gap-2">
              <span className="font-mono text-slate-200">{pkg.name}</span>
              <Badge tone="teal">{pkg.version}</Badge>
              <Badge tone={pkg.manager === "pnpm" ? "violet" : "slate"}>{pkg.manager}</Badge>
              {pkg.mcpCapable && <Badge tone="sky" icon="mcp">MCP 相关</Badge>}
            </div>
            {pkg.location && (
              <div className="mono mt-1 truncate text-[10.5px]" title={pkg.location}>
                {shortenPath(pkg.location, 60)}
              </div>
            )}
          </div>
        )}
        {plan && (
          <div className="rounded-md border border-rose-800/60 bg-rose-950 p-3">
            <pre className="mono whitespace-pre-wrap break-all text-[10.5px] text-slate-300">
              {plan.managerPath} {plan.args.join(" ")}
            </pre>
            <p className="mt-1.5 text-[11px] text-slate-400">{plan.message}</p>
          </div>
        )}
        {result && (
          <div className={`rounded-md border p-3 ${result.ok ? "border-brand-800 bg-brand-900" : "border-rose-800/70 bg-rose-950"}`}>
            <div className="flex items-center gap-2 text-xs">
              <StatusDot state={result.ok ? "ok" : "error"} />
              <span className={result.ok ? "text-brand-400" : "text-rose-300"}>
                {result.summary || result.title}
              </span>
            </div>
          </div>
        )}
      </div>
    </Modal>
  );
}

/* ---------------------------------------------------------------- 页面 */

export function NpmPage() {
  const snapshot = useApp((s) => s.snapshot);
  const scan = useApp((s) => s.scan);
  const scanning = useApp((s) => s.scanning);
  const setBanner = useApp((s) => s.setBanner);
  const [query, setQuery] = useState("");
  const [manager, setManager] = useState("all");
  const [onlyMcp, setOnlyMcp] = useState(false);
  const [installOpen, setInstallOpen] = useState(false);
  const [removing, setRemoving] = useState<NpmPackage | null>(null);
  const [outdated, setOutdated] = useState<Map<string, NpmOutdated> | null>(null);
  const [outdatedBusy, setOutdatedBusy] = useState(false);
  const [upgrading, setUpgrading] = useState<string | null>(null);

  const packages = snapshot?.npmPackages ?? [];

  const checkUpdates = async () => {
    setOutdatedBusy(true);
    try {
      const list = await api.npmOutdated("npm");
      setOutdated(new Map(list.map((item) => [item.name, item])));
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setOutdatedBusy(false);
    }
  };

  const upgrade = async (pkg: NpmOutdated) => {
    setUpgrading(pkg.name);
    try {
      const result = await api.npmInstallRun("npm", [pkg.name]);
      if (result.ok) {
        setBanner(`已升级 ${pkg.name}：${result.summary}`);
        setOutdated((prev) => {
          const next = prev != null ? new Map(prev) : new Map();
          next.delete(pkg.name);
          return next;
        });
        void scan();
      } else {
        setBanner(result.steps.map((s) => s.message).join("；"));
      }
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setUpgrading(null);
    }
  };

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
          <div className="flex items-center gap-2">
            <div className="w-44">
              <SearchInput value={query} onChange={setQuery} placeholder="搜索包名 / 版本…" />
            </div>
            <button
              type="button"
              className="btn-ghost btn-sm"
              onClick={() => void checkUpdates()}
              disabled={outdatedBusy}
              title="npm outdated -g --json：只读检查哪些全局包有新版本"
            >
              <Icon name="refresh" className={`h-3.5 w-3.5 ${outdatedBusy ? "animate-spin" : ""}`} />
              {outdatedBusy ? "检查中…" : "检查更新"}
            </button>
            <button
              type="button"
              className="btn-primary btn-sm"
              onClick={() => setInstallOpen(true)}
              title="T3：先看计划（npm install -g / pnpm add -g）再执行"
            >
              <Icon name="plus" className="h-3.5 w-3.5" />
              安装包
            </button>
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
          <div className="w-56 rounded-md border border-ink-800 bg-ink-900 px-3">
            <Toggle
              checked={onlyMcp}
              onChange={setOnlyMcp}
              label="只看与 MCP 相关"
              hint="包名包含 mcp 或 @modelcontextprotocol"
            />
          </div>
          <button
            type="button"
            className="btn-ghost btn-sm ml-auto"
            onClick={() => void scan()}
            disabled={scanning}
            title="安装/卸载后重新扫描以刷新列表"
          >
            <Icon name="refresh" className={`h-3.5 w-3.5 ${scanning ? "animate-spin" : ""}`} />
            重新扫描
          </button>
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
            action={
              packages.length === 0 ? (
                <button type="button" className="btn-primary mt-2" onClick={() => setInstallOpen(true)}>
                  <Icon name="plus" className="h-4 w-4" />
                  安装第一个包
                </button>
              ) : undefined
            }
          />
        ) : (
          <div className="overflow-hidden rounded-md border border-ink-800/70">
            <table className="w-full border-collapse">
              <thead>
                <tr>
                  <th className="table-head px-3 py-2">包名</th>
                  <th className="table-head w-28 px-3 py-2">版本</th>
                  <th className="table-head w-24 px-3 py-2">管理器</th>
                  <th className="table-head w-32 px-3 py-2">标记</th>
                  <th className="table-head px-3 py-2">安装位置</th>
                  <th className="table-head w-24 px-3 py-2">操作</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((pkg) => {
                  const update = outdated?.get(pkg.name);
                  return (
                    <tr key={`${pkg.manager}-${pkg.name}`} className="hover:bg-ink-800">
                      <td className="table-cell px-3">
                        <div className="flex items-center gap-2">
                          <Icon name="npm" className="h-3.5 w-3.5 shrink-0 text-amber-300" />
                          <span className="truncate font-mono text-xs text-slate-200">{pkg.name}</span>
                        </div>
                      </td>
                      <td className="table-cell px-3 font-mono text-[11px] text-brand-400">
                        <div className="flex items-center gap-1.5">
                          {pkg.version}
                          {update && (
                            <Badge tone="amber">→ {update.latest}</Badge>
                          )}
                        </div>
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
                      <td className="table-cell px-3">
                        <div className="flex items-center gap-1.5">
                          {update && (
                            <button
                              type="button"
                              className="btn border border-amber-500/40 btn-sm text-amber-300 hover:bg-amber-950"
                              onClick={() => void upgrade(update)}
                              disabled={upgrading === pkg.name}
                              title={`npm install -g ${pkg.name}（装最新版）`}
                            >
                              <Icon
                                name="refresh"
                                className={`h-3.5 w-3.5 ${upgrading === pkg.name ? "animate-spin" : ""}`}
                              />
                              {upgrading === pkg.name ? "升级中" : "升级"}
                            </button>
                          )}
                          <button
                            type="button"
                            className="btn border border-rose-500/40 btn-sm text-rose-300 hover:bg-rose-950"
                            onClick={() => setRemoving(pkg)}
                            title="全局卸载（T3：先看计划）"
                          >
                            <Icon name="close" className="h-3.5 w-3.5" />
                          </button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </SectionCard>

      <div className="grid gap-3 md:grid-cols-3">
        <Card className="border-dashed">
          <div className="flex items-center gap-2">
            <Icon name="plus" className="h-4 w-4 text-accent-400" />
            <span className="text-sm text-slate-200">安装 / 卸载</span>
            <Badge tone="teal" className="ml-auto">
              已实现
            </Badge>
          </div>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
            「安装包」先展示将执行的 <span className="font-mono">npm install -g</span> /
            <span className="font-mono"> pnpm add -g</span>（T3 计划确认），完成后由{" "}
            <span className="font-mono">ls -g</span> 读回版本落库；卸载走包管理器，可随时重装。
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

      <InstallDialog
        open={installOpen}
        onClose={() => setInstallOpen(false)}
        onDone={() => void scan()}
      />
      <RemoveDialog pkg={removing} onClose={() => setRemoving(null)} onDone={() => void scan()} />
    </div>
  );
}
