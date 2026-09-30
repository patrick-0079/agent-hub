/** 密钥保险库：DPAPI 加密密钥的完整管理（清单 / 新增 / 查看 / 删除 / 健康检查）。 */

import React, { useEffect, useState } from "react";
import { Icon } from "../components/Icon";
import {
  Badge,
  Card,
  Empty,
  Modal,
  SectionCard,
  StatusDot,
  useCopy,
} from "../components/ui";
import { api, describeError } from "../lib/api";
import { useReveal } from "../lib/hooks";
import { useApp } from "../lib/store";
import type { VaultKeyInfo, VaultStatus } from "../lib/types";

export function VaultPage() {
  const setBanner = useApp((s) => s.setBanner);
  const reveal = useReveal();
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [keys, setKeys] = useState<VaultKeyInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");

  /* 新增密钥 */
  const [addOpen, setAddOpen] = useState(false);
  const [newId, setNewId] = useState("");
  const [newSecret, setNewSecret] = useState("");
  const [busy, setBusy] = useState(false);

  /* 显示明文（二次确认） */
  const [revealTarget, setRevealTarget] = useState<VaultKeyInfo | null>(null);
  const [revealConfirming, setRevealConfirming] = useState(false);
  const [revealed, setRevealed] = useState<string | null>(null);
  const [copied, copy] = useCopy();

  /* 删除确认 */
  const [removeTarget, setRemoveTarget] = useState<VaultKeyInfo | null>(null);

  const load = () => {
    setLoading(true);
    Promise.all([api.vaultStatus(), api.vaultKeys()])
      .then(([s, k]) => {
        setStatus(s);
        setKeys(k);
      })
      .catch((error) => setBanner(describeError(error)))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    load();
  }, []);

  const filtered = keys.filter((k) => {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return (
      k.id.toLowerCase().includes(q) ||
      (k.linkedProvider ?? "").toLowerCase().includes(q)
    );
  });

  const providerKeys = filtered.filter((k) => k.linkedProvider != null);
  const standaloneKeys = filtered.filter((k) => k.linkedProvider == null);
  const unhealthy = keys.filter((k) => !k.healthy);

  const addKey = async () => {
    setBusy(true);
    try {
      setKeys(await api.vaultKeySet(newId, newSecret));
      setAddOpen(false);
      setNewId("");
      setNewSecret("");
      load();
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const confirmReveal = async () => {
    if (!revealTarget) return;
    try {
      setRevealed(await api.vaultReveal(revealTarget.id));
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  const removeKey = async () => {
    if (!removeTarget) return;
    setBusy(true);
    try {
      setKeys(await api.vaultKeyRemove(removeTarget.id));
      setRemoveTarget(null);
      load();
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const renderKeyRow = (k: VaultKeyInfo) => (
    <div
      key={k.id}
      className="flex flex-wrap items-center gap-3 rounded-md border border-ink-800 bg-ink-900 px-3.5 py-3"
    >
      <StatusDot state={k.healthy ? "ok" : "error"} />
      <span className="min-w-0 flex-1">
        <span className="flex flex-wrap items-center gap-2">
          <span className="mono truncate text-xs text-slate-200">{k.id}</span>
          {k.linkedProvider && (
            <Badge tone="teal">供应商 {k.linkedProvider}</Badge>
          )}
          {!k.healthy && <Badge tone="rose">无法解密</Badge>}
        </span>
        <span className="mono mt-0.5 block truncate text-[11px] text-slate-500">
          {k.healthy ? (k.masked ?? "—") : "密文与当前用户不匹配（可能来自其它用户/机器）"}
        </span>
      </span>
      <div className="flex shrink-0 items-center gap-1.5">
        {k.healthy && (
          <button
            type="button"
            className="btn-ghost btn-sm"
            onClick={() => {
              setRevealTarget(k);
              setRevealConfirming(false);
              setRevealed(null);
            }}
            title="显式查看明文（二次确认，不写日志）"
          >
            <Icon name="lock" className="h-3.5 w-3.5" />
            显示明文
          </button>
        )}
        <button
          type="button"
          className="btn border border-rose-500/40 btn-sm text-rose-300 hover:bg-rose-950"
          onClick={() => setRemoveTarget(k)}
          title="从保险库删除该密钥"
        >
          <Icon name="close" className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  );

  return (
    <div className="space-y-4">
      {/* 状态 */}
      {status && (
        <Card className={status.healthy ? "" : "border-rose-500/40"}>
          <div className="flex flex-wrap items-center gap-3">
            <Icon
              name="vault"
              className={`h-5 w-5 shrink-0 ${status.healthy ? "text-slate-400" : "text-rose-300"}`}
            />
            <div className="min-w-0 flex-1">
              <div className="text-sm text-slate-200">
                保险库 · {status.count} 个密钥 ·{" "}
                {status.healthy ? "全部可解密" : `${unhealthy.length} 个无法解密`}
              </div>
              <div className="mt-0.5 text-[11px] leading-relaxed text-slate-500">
                {status.message}
              </div>
            </div>
            <button
              type="button"
              className="btn-ghost btn-sm shrink-0"
              onClick={() => reveal(status.path)}
            >
              <Icon name="folder" className="h-3.5 w-3.5" />
              打开保险库目录
            </button>
          </div>
        </Card>
      )}

      <SectionCard
        title="密钥清单"
        subtitle={`共 ${keys.length} 个 —— 明文永不落库；密文绑定当前 Windows 用户（DPAPI）`}
        action={
          <div className="flex items-center gap-2">
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="搜索标识 / 供应商…"
              className="input w-40 py-1.5 text-xs"
            />
            <button
              type="button"
              className="btn-primary btn-sm"
              onClick={() => setAddOpen(true)}
            >
              <Icon name="plus" className="h-3.5 w-3.5" />
              新增密钥
            </button>
          </div>
        }
        bodyClassName="space-y-4"
      >
        {keys.length === 0 ? (
          <Empty
            icon="vault"
            title={loading ? "正在读取…" : "保险库是空的"}
            description="在「模型供应商」录入 API Key 时会自动写入这里；也可以直接新增独立密钥（供 MCP 环境变量等引用）。"
            action={
              <button type="button" className="btn-primary mt-2" onClick={() => setAddOpen(true)}>
                <Icon name="plus" className="h-4 w-4" />
                新增第一个密钥
              </button>
            }
          />
        ) : (
          <>
            {providerKeys.length > 0 && (
              <div>
                <div className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-slate-600">
                  供应商密钥（{providerKeys.length}）
                </div>
                <div className="space-y-2">{providerKeys.map(renderKeyRow)}</div>
              </div>
            )}
            {standaloneKeys.length > 0 && (
              <div>
                <div className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-slate-600">
                  独立密钥（{standaloneKeys.length}）
                </div>
                <div className="space-y-2">{standaloneKeys.map(renderKeyRow)}</div>
              </div>
            )}
            {providerKeys.length === 0 && standaloneKeys.length === 0 && (
              <Empty icon="search" title="没有匹配的密钥" />
            )}
          </>
        )}
      </SectionCard>

      {/* 新增密钥 */}
      <Modal
        open={addOpen}
        onClose={() => setAddOpen(false)}
        title="新增密钥"
        subtitle="DPAPI 加密写入 vault.json（绑定当前用户）；密钥标识供后续引用"
        width="max-w-xl"
        footer={
          <>
            <button type="button" className="btn-ghost" onClick={() => setAddOpen(false)} disabled={busy}>
              取消
            </button>
            <button
              type="button"
              className="btn-primary"
              onClick={() => void addKey()}
              disabled={busy || !newId.trim() || !newSecret.trim()}
            >
              {busy ? "写入中…" : "加密写入"}
            </button>
          </>
        }
      >
        <div className="space-y-3.5">
          <div>
            <label className="text-xs text-slate-400">密钥标识</label>
            <input
              value={newId}
              onChange={(e) => setNewId(e.target.value)}
              placeholder="如 provider:my-provider 或 mcp:github-token"
              className="input mt-1.5 font-mono text-xs"
            />
            <p className="mt-1 text-[11px] text-slate-500">
              供应商密钥用 <span className="font-mono">provider:&lt;名称&gt;</span>；
              自定义用途随意命名（MCP 环境变量可用 <span className="font-mono">%</span> 引用展开）。
            </p>
          </div>
          <div>
            <label className="text-xs text-slate-400">密钥值</label>
            <input
              type="password"
              value={newSecret}
              onChange={(e) => setNewSecret(e.target.value)}
              placeholder="sk-…"
              className="input mt-1.5 font-mono text-xs"
              autoComplete="off"
            />
          </div>
        </div>
      </Modal>

      {/* 显示明文（二次确认） */}
      <Modal
        open={revealTarget != null}
        onClose={() => {
          setRevealTarget(null);
          setRevealed(null);
          setRevealConfirming(false);
        }}
        title={`查看明文：${revealTarget?.id ?? ""}`}
        subtitle="明文只在本次弹窗内显示，关闭即清除；不写入任何日志"
        width="max-w-xl"
        footer={
          revealed ? (
            <button
              type="button"
              className="btn-primary"
              onClick={() => {
                setRevealTarget(null);
                setRevealed(null);
              }}
            >
              关闭并清除
            </button>
          ) : revealConfirming ? (
            <>
              <button
                type="button"
                className="btn-ghost"
                onClick={() => setRevealConfirming(false)}
              >
                返回
              </button>
              <button type="button" className="btn-primary" onClick={() => void confirmReveal()}>
                确认显示明文
              </button>
            </>
          ) : (
            <button
              type="button"
              className="btn-primary"
              onClick={() => setRevealConfirming(true)}
            >
              我已了解风险，继续
            </button>
          )
        }
      >
        <div className="space-y-3">
          {revealTarget && (
            <div className="rounded-md border border-ink-800 bg-ink-900 px-3 py-2">
              <div className="mono text-[11px] text-slate-500">当前掩码</div>
              <div className="mono mt-0.5 text-xs text-slate-300">
                {revealTarget.masked ?? "—"}
              </div>
            </div>
          )}
          {revealConfirming && !revealed && (
            <p className="text-xs leading-relaxed text-amber-300">
              明文将显示在下方；请确认周围无人 / 无录屏。此操作不会被记录。
            </p>
          )}
          {revealed && (
            <div>
              <div className="mb-1.5 flex items-center justify-between text-xs">
                <span className="text-slate-300">明文（关闭弹窗即清除）</span>
                <button
                  type="button"
                  className="btn-ghost btn-sm"
                  onClick={() => copy(revealed)}
                >
                  {copied === revealed ? "已复制" : "复制"}
                </button>
              </div>
              <pre className="overflow-auto rounded-md border border-ink-800 bg-ink-950 p-3 font-mono text-[11px] leading-relaxed break-all text-slate-200">
                {revealed}
              </pre>
            </div>
          )}
        </div>
      </Modal>

      {/* 删除确认 */}
      <Modal
        open={removeTarget != null}
        onClose={() => setRemoveTarget(null)}
        title={`删除密钥：${removeTarget?.id ?? ""}`}
        subtitle="密文将从 vault.json 移除（不可恢复）"
        width="max-w-lg"
        footer={
          <>
            <button type="button" className="btn-ghost" onClick={() => setRemoveTarget(null)} disabled={busy}>
              取消
            </button>
            <button
              type="button"
              className="btn border border-rose-500/40 text-rose-300 hover:bg-rose-950"
              onClick={() => void removeKey()}
              disabled={busy}
            >
              {busy ? "删除中…" : "确认删除"}
            </button>
          </>
        }
      >
        {removeTarget?.linkedProvider && (
          <p className="text-xs leading-relaxed text-amber-300">
            该密钥关联供应商「{removeTarget.linkedProvider}」——删除后该供应商的分发与
            连通性测试将提示「密钥不在保险库中」，需重新录入。
          </p>
        )}
        {removeTarget && !removeTarget.linkedProvider && (
          <p className="text-xs leading-relaxed text-slate-400">
            这是一个独立密钥，删除后引用它的地方（如有）将无法解密。
          </p>
        )}
      </Modal>
    </div>
  );
}
