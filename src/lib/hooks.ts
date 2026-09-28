/** 页面通用 Hook。 */

import { useCallback } from "react";
import { api, describeError } from "./api";
import { useApp } from "./store";

/** 在资源管理器中打开路径，失败时通过全局提示条反馈。 */
export function useReveal(): (path: string) => void {
  const setBanner = useApp((s) => s.setBanner);
  return useCallback(
    (path: string) => {
      api.revealPath(path).catch((error) => setBanner(describeError(error)));
    },
    [setBanner],
  );
}

export const CATEGORY_LABEL: Record<string, string> = {
  runtime: "运行时",
  "node-pm": "Node 包管理",
  python: "Python 工具链",
  vcs: "版本控制",
  container: "容器",
  editor: "编辑器",
  "agent-cli": "Agent 命令行程序",
};

export const MANAGER_LABEL: Record<string, string> = {
  conda: "Conda",
  uv: "uv",
  venv: "venv / virtualenv",
};

export const KIND_LABEL: Record<string, string> = {
  "api-key": "API Key",
  "base-url": "Base URL",
  model: "模型",
  credential: "凭据",
};

export const TRANSPORT_LABEL: Record<string, string> = {
  stdio: "stdio",
  http: "http",
  sse: "sse",
  unknown: "未知",
};

export const LINK_LABEL: Record<string, string> = {
  junction: "目录链接",
  symlink: "符号链接",
};