/** 密钥保险库：API Key 加密存储与验证（M1）。 */

import React from "react";
import { PlannedPage } from "../components/ui";

export function VaultPage() {
  return (
    <PlannedPage
      icon="vault"
      title="密钥保险库"
      milestone="M1"
      summary="模型供应商的 API Key 统一加密存放，明文永不落库、永不出现在导出文件中；需要时只在渲染模版的瞬间解密注入。"
      bullets={[
        {
          title: "系统级加密",
          detail: "Windows 使用 DPAPI（绑定当前用户），macOS 用 Keychain，Linux 用 libsecret，避免明文存储。",
        },
        {
          title: "默认脱敏",
          detail: "列表、日志、diff 一律显示掩码；点击「显示明文」需要二次确认且不写入任何日志。",
        },
        {
          title: "验证状态可视化",
          detail: "每个 Key 记录最近一次连通性验证结果与时间，失效的 Key 在资源页与仪表盘高亮提示。",
        },
        {
          title: "导出保护",
          detail: "导出整套环境时自动剔除密钥，只保留引用名；换机迁移后重新录入即可。",
        },
      ]}
    />
  );
}