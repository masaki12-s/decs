# decs 🤿
decs (Dive into ECS) は、Amazon ECSコンテナへの ecs exec を圧倒的に簡単にするRust製CLIツールです。


# 🚀 特徴
インタラクティブな選択: クラスター、サービス、タスクを一覧から選択するだけで接続。

オートフィルタリング: 実行中のタスク（RUNNING）のみを表示し、目的のコンテナへ即座にアクセス。

SSM連携チェック: 接続に必要なセッションマネージャープラグインや権限の有無を自動で確認。

# 📦 インストール
Binary (Recommended)
Releases からお使いのOSに合わせたバイナリをダウンロードし、実行パスの通った場所に配置してください。

Cargo
```bash
cargo install decs
```

# 💡 使い方
ターミナルで decs と入力するだけで、対話形式の選択画面が始まります。

```bash
decs

? Select Cluster:
> production-cluster
  staging-cluster

? Select Service:
  api-service
> worker-service

? Select Task:
> 1234567890abcdef (RUNNING) - Last status: RUNNING

? Select Container:
> app-container
  log-router

Connecting to container...

# コンテナ内に入りました！
node@container:/app$
```

オプション指定
特定のクラスターを直接指定して開始することも可能です。

```bash
decs -c my-cluster -s my-service
```

# 🛠 前提条件
decs を使用するには、以下のセットアップが必要です。

AWS CLI: 設定済みの認証情報（~/.aws/credentials）

Session Manager Plugin: AWS公式のインストールガイド を参照してください。

ECS Execute Command の有効化: 対象のサービス/タスクで enableExecuteCommand が true に設定されている必要があります。

# 📝 ライセンス
MIT License