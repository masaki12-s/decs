# decs 🤿
decs (Dive into ECS) は、Amazon ECSコンテナへの `ecs execute-command` を圧倒的に簡単にするRust製CLIツールです。


## 🚀 特徴
- **インタラクティブな選択**: クラスター、サービス、タスク、コンテナを一覧から選択するだけで接続
- **オートフィルタリング**: 実行中のタスク（RUNNING）のみを表示し、目的のコンテナへ即座にアクセス
- **SSM連携チェック**: 接続に必要な Session Manager Plugin の有無を自動で確認
- **タスク情報の確認**: `--inspect` モードでタスクのステータスやネットワーク情報をテーブル表示
- **柔軟なオプション指定**: クラスター、サービス、タスク、コンテナをCLI引数で直接指定可能

## 📦 インストール

### Cargo
```bash
cargo install decs
```

### ソースからビルド
```bash
git clone https://github.com/masaki12-s/decs.git
cd decs
cargo build --release
```

## 💡 使い方

### インタラクティブモード
ターミナルで `decs` と入力するだけで、対話形式の選択画面が始まります。

```bash
decs

? Select Cluster:
> production-cluster
  staging-cluster

? Select Service:
  api-service
> worker-service

? Select Task:
> 1234567890abcdef (RUNNING) containers: app, log-router

? Select Container:
> app
  log-router

Connecting to cluster=production-cluster, service=worker-service, task=1234567890abcdef, container=app ...
```

### オプション指定
クラスターやサービスなどをCLI引数で直接指定して、プロンプトをスキップできます。

```bash
# クラスターとサービスを指定（タスクとコンテナは対話選択）
decs -c my-cluster -s my-service

# 全パラメータを指定（対話なしで即接続）
decs -c my-cluster -s my-service -t my-task-id -n my-container

# 実行コマンドを変更（デフォルトは /bin/sh）
decs -x /bin/bash

# AWS プロファイル・リージョンを指定
decs --profile staging --region ap-northeast-1
```

### inspect モード
`--inspect` フラグを付けると、コンテナに接続せずにタスクの詳細情報をテーブル形式で表示します。

```bash
decs --inspect -c my-cluster -s my-service

Task Status (cluster: my-cluster, service: my-service)
Task ID               Status   Private IP  Public IP  ENI          Containers
--------------------  -------  ----------  ---------  -----------  ----------
1234567890abcdef      RUNNING  10.0.0.10   -          eni-abc123   app,sidecar
```

### CLIオプション一覧

| オプション | 短縮 | 説明 | デフォルト |
|---|---|---|---|
| `--cluster` | `-c` | クラスター名（指定時はプロンプトをスキップ） | - |
| `--service` | `-s` | サービス名（指定時はプロンプトをスキップ） | - |
| `--task` | `-t` | タスクID（指定時はプロンプトをスキップ） | - |
| `--container` | `-n` | コンテナ名（指定時はプロンプトをスキップ） | - |
| `--command` | `-x` | コンテナ内で実行するコマンド | `/bin/sh` |
| `--profile` | - | AWS プロファイル名 | - |
| `--region` | - | AWS リージョン | - |
| `--inspect` | - | タスク詳細を表示して終了（exec なし） | - |

## 🛠 前提条件
decs を使用するには、以下のセットアップが必要です。

- **AWS CLI**: 設定済みの認証情報（`~/.aws/credentials` または環境変数）
- **Session Manager Plugin**: [AWS公式のインストールガイド](https://docs.aws.amazon.com/systems-manager/latest/userguide/session-manager-working-with-install-plugin.html)を参照してください
- **ECS Execute Command の有効化**: 対象のサービス/タスクで `enableExecuteCommand` が `true` に設定されている必要があります

## 📝 ライセンス
MIT License