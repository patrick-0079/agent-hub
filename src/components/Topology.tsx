/** 拓扑视图：Agent 目标 ⇄ Profile 层 ⇄ 资源池 的关系可视化（SVG，无第三方依赖）。 */

import React from "react";
import type { Route, ScanSnapshot } from "../lib/types";

interface Props {
  snapshot: ScanSnapshot;
  onNavigate: (route: Route) => void;
}

const W = 1000;
const H = 470;
const HUB = { x: W / 2, y: H / 2 };
const COL_W = 214;
const ROW_H = 52;
const LEFT_X = 22;
const RIGHT_X = W - 22 - COL_W;
const BAND = { x: 340, y: 34, w: 320, h: H - 68 };

function truncate(text: string, max = 17): string {
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

function bezier(x1: number, y1: number, x2: number, y2: number): string {
  const dx = Math.abs(x2 - x1) * 0.55;
  return `M ${x1} ${y1} C ${x1 + dx} ${y1}, ${x2 - dx} ${y2}, ${x2} ${y2}`;
}

function rowCenters(count: number): number[] {
  const usable = H - 90;
  const spacing = Math.min(74, usable / Math.max(count, 1));
  const total = spacing * count;
  const start = HUB.y - total / 2 + spacing / 2;
  return Array.from({ length: count }, (_, i) => start + i * spacing);
}

export function Topology({ snapshot, onNavigate }: Props) {
  // 宿主应用（VS Code 等）不是 Agent，单独在 Agent 页展示，不进拓扑
  const agents = snapshot.agents.filter((a) => a.installed && a.kind !== "host").slice(0, 6);
  const agentYs = rowCenters(Math.max(agents.length, 1));

  const resources: {
    route: Route;
    label: string;
    count: number;
    color: string;
    unit: string;
  }[] = [
    {
      route: "providers",
      label: "模型供应商",
      count: snapshot.providerHints.length,
      color: "#a78bfa",
      unit: "条线索",
    },
    { route: "skills", label: "Skills", count: snapshot.skills.length, color: "#2dd4bf", unit: "个" },
    {
      route: "mcp",
      label: "MCP 服务器",
      count: snapshot.mcpServers.length,
      color: "#7dd3fc",
      unit: "个",
    },
    {
      route: "npm",
      label: "npm 全局包",
      count: snapshot.npmPackages.length,
      color: "#fbbf24",
      unit: "个",
    },
    {
      route: "python",
      label: "Python 环境",
      count: snapshot.pythonEnvs.length,
      color: "#34d399",
      unit: "个",
    },
  ];
  const resourceYs = rowCenters(resources.length);

  const toolReady = snapshot.executables.filter((e) => e.found).length;
  const toolTotal = snapshot.executables.length;

  return (
    <div className="w-full">
      <svg viewBox={`0 0 ${W} ${H}`} className="h-auto w-full" preserveAspectRatio="xMidYMid meet">
        <defs>
          <radialGradient id="hubGlow" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="#2dd4bf" stopOpacity="0.55" />
            <stop offset="70%" stopColor="#2dd4bf" stopOpacity="0.12" />
            <stop offset="100%" stopColor="#2dd4bf" stopOpacity="0" />
          </radialGradient>
          <linearGradient id="hubFill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#0f1c2b" />
            <stop offset="100%" stopColor="#0b1220" />
          </linearGradient>
        </defs>

        {/* Profile 层（M1 起启用） */}
        <rect
          x={BAND.x}
          y={BAND.y}
          width={BAND.w}
          height={BAND.h}
          rx="20"
          fill="#8b5cf6"
          fillOpacity="0.04"
          stroke="#8b5cf6"
          strokeOpacity="0.35"
          strokeWidth="1.2"
          strokeDasharray="7 6"
        />
        <text
          x={BAND.x + BAND.w / 2}
          y={BAND.y + 20}
          textAnchor="middle"
          fill="#a78bfa"
          fontSize="11.5"
          fontWeight="600"
        >
          Profile 层 · M1 起启用
        </text>
        <text x={BAND.x + BAND.w / 2} y={BAND.y + BAND.h - 12} textAnchor="middle" fill="#6b7280" fontSize="10.5">
          资源组合 → 一键同步到各 Agent
        </text>

        {/* 连线：Agent → 枢纽 */}
        {agents.map((agent, i) => (
          <path
            key={`edge-a-${agent.id}`}
            d={bezier(LEFT_X + COL_W, agentYs[i], HUB.x - 66, HUB.y)}
            fill="none"
            stroke={agent.accent}
            strokeOpacity="0.5"
            strokeWidth="1.4"
          />
        ))}
        {/* 连线：枢纽 → 资源 */}
        {resources.map((res, i) => (
          <path
            key={`edge-r-${res.route}`}
            d={bezier(HUB.x + 66, HUB.y, RIGHT_X, resourceYs[i])}
            fill="none"
            stroke={res.color}
            strokeOpacity="0.45"
            strokeWidth="1.4"
          />
        ))}

        {/* 枢纽 */}
        <circle cx={HUB.x} cy={HUB.y} r="104" fill="url(#hubGlow)" />
        <circle
          cx={HUB.x}
          cy={HUB.y}
          r="66"
          fill="url(#hubFill)"
          stroke="#2dd4bf"
          strokeOpacity="0.55"
          strokeWidth="1.4"
        />
        <text x={HUB.x} y={HUB.y - 10} textAnchor="middle" fill="#e2e8f0" fontSize="15" fontWeight="700">
          本机环境
        </text>
        <text x={HUB.x} y={HUB.y + 9} textAnchor="middle" fill="#5eead4" fontSize="11">
          {snapshot.host.hostname}
        </text>
        <text x={HUB.x} y={HUB.y + 27} textAnchor="middle" fill="#64748b" fontSize="10">
          工具链 {toolReady}/{toolTotal}
        </text>

        {/* Agent 节点 */}
        {agents.length === 0 && (
          <g>
            <rect
              x={LEFT_X}
              y={HUB.y - ROW_H / 2}
              width={COL_W}
              height={ROW_H}
              rx="12"
              fill="#0f1420"
              stroke="#2a3446"
              strokeDasharray="5 4"
            />
            <text x={LEFT_X + COL_W / 2} y={HUB.y + 4} textAnchor="middle" fill="#64748b" fontSize="11.5">
              未发现已安装的 Agent
            </text>
          </g>
        )}
        {agents.map((agent, i) => {
          const y = agentYs[i] - ROW_H / 2;
          return (
            <g
              key={agent.id}
              className="cursor-pointer"
              onClick={() => onNavigate("agents")}
              role="button"
            >
              <rect
                x={LEFT_X}
                y={y}
                width={COL_W}
                height={ROW_H}
                rx="12"
                fill="#0f1420"
                stroke={agent.accent}
                strokeOpacity="0.55"
                strokeWidth="1.2"
              />
              <rect x={LEFT_X} y={y + 8} width="3" height={ROW_H - 16} rx="1.5" fill={agent.accent} />
              <text x={LEFT_X + 16} y={y + 21} fill="#e2e8f0" fontSize="12.5" fontWeight="600">
                {truncate(agent.name)}
              </text>
              <text x={LEFT_X + 16} y={y + 38} fill="#64748b" fontSize="10.5">
                MCP {agent.mcpCount} · Skill {agent.skillCount} · {agent.vendor}
              </text>
            </g>
          );
        })}

        {/* 资源节点 */}
        {resources.map((res, i) => {
          const y = resourceYs[i] - ROW_H / 2;
          return (
            <g
              key={res.route}
              className="cursor-pointer"
              onClick={() => onNavigate(res.route)}
              role="button"
            >
              <rect
                x={RIGHT_X}
                y={y}
                width={COL_W}
                height={ROW_H}
                rx="12"
                fill="#0f1420"
                stroke="#2a3446"
                strokeWidth="1.1"
              />
              <text x={RIGHT_X + 16} y={y + 22} fill="#e2e8f0" fontSize="12.5" fontWeight="600">
                {res.label}
              </text>
              <text x={RIGHT_X + 16} y={y + 38} fill="#64748b" fontSize="10.5">
                {res.count} {res.unit}
              </text>
              <text
                x={RIGHT_X + COL_W - 16}
                y={y + 31}
                textAnchor="end"
                fill={res.color}
                fontSize="17"
                fontWeight="700"
              >
                {res.count}
              </text>
            </g>
          );
        })}
      </svg>

      {/* 图例 */}
      <div className="mt-1 flex flex-wrap items-center gap-x-5 gap-y-1.5 px-1 text-[11px] text-slate-500">
        <span className="flex items-center gap-1.5">
          <span className="h-0.5 w-6 rounded bg-brand-500/70" />
          已发现的 Agent 目标
        </span>
        <span className="flex items-center gap-1.5">
          <span className="h-0.5 w-6 rounded bg-accent-500/70" />
          Profile 层（规划中）
        </span>
        <span className="flex items-center gap-1.5">
          <span className="h-2 w-2 rounded-full border border-brand-500/70 bg-brand-500/20" />
          点击任意节点可跳转到对应功能页
        </span>
      </div>
    </div>
  );
}