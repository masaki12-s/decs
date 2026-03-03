# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**decs** (Dive into ECS) - AWS ECS コンテナにインタラクティブにアクセスするための Rust CLI ツール。クラスタ、サービス、タスク、コンテナを対話的に選択し、`aws ecs execute-command` を実行する。

## Build & Test Commands

```bash
cargo build                # デバッグビルド
cargo build --release      # リリースビルド
cargo test                 # 全テスト実行
cargo test --lib           # lib.rs のテストのみ実行
cargo test <test_name>     # 単一テスト実行 (例: cargo test builds_plan_with_prompts)
cargo clippy               # Lint
cargo fmt                  # フォーマット
cargo run                  # 実行 (インタラクティブモード)
cargo run -- --inspect     # タスク情報の表示のみ (exec なし)
```

## Architecture

2ファイル構成: `src/main.rs` (エントリポイント) と `src/lib.rs` (コアロジック)。

### トレイトベースの抽象化

テスト容易性のため、AWS API とユーザー操作をトレイトで抽象化している:

- **`EcsApi`** - AWS ECS データアクセス (`list_clusters`, `list_services`, `list_running_tasks`, `list_containers`, `describe_tasks`)
- **`Prompter`** - ユーザー対話 (`select_cluster`, `select_service`, `select_task`, `select_container`)

実装:
- `AwsEcsApi` - 実際の AWS SDK を使用した実装
- `InquirePrompter` - inquire クレートによるターミナル UI

### 主要な型

- `AppConfig` - CLI 引数をパースした設定 (cluster/service/task/container は全て `Option<String>`)
- `ExecutionPlan` - 実行に必要な情報を全て保持する構造体
- `TaskInfo` - タスク ID、ステータス、コンテナ名、ネットワーク情報を保持

### 処理フロー

1. CLI 引数 → `AppConfig` 変換
2. `build_plan()`: 未指定のパラメータをインタラクティブに解決し `ExecutionPlan` を構築
3. `execute_plan()`: `aws ecs execute-command` を子プロセスとして実行
4. `--inspect` モード: タスク詳細をテーブル形式で表示して終了

### 非同期トレイトの実装

`Pin<Box<dyn Future>>` (`BoxFuture` 型エイリアス) を使用。async-trait クレートは未使用。

### テスト

`lib.rs` 末尾に `FakeEcs` と `FakePrompter` のモック実装があり、AWS 接続なしでテスト可能。テストは tokio の非同期テストとして実行。
