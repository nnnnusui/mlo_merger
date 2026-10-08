# MLO Merger

[![SUSHI-WARE LICENSE](https://img.shields.io/badge/license-SUSHI--WARE%F0%9F%8D%A3-blue.svg)](https://github.com/MakeNowJust/sushi-ware)

[English](../Readme.md) | [設計](ARCHITECTURE.md) | [実装計画](TODO.md)

FiveM の YMAP と YBN コリジョンの変更を vanilla データにマージし、
生成したストリームファイルを配置するツールです。

## パイプラインを実行する

```bash
mlo_merger
mlo_merger -i resources -o merged_mlo --game-dir /mnt/gtav
```

操作フラグを省略すると、vanilla アーカイブがない場合だけ抽出し、
vanilla-cache、source-cache、マージ、配置の順に実行します。
変更のないステージは再利用します。
`--game-dir` の既定値は `/mnt/gtav` で、vanilla の抽出が必要な場合だけ使います。

| 用途 | 既定パス | オプション |
|---|---|---|
| ソースリソース | `asset/source` | `-i` |
| 生の vanilla アーカイブ | `asset/vanilla` | `--vanilla` |
| vanilla 派生キャッシュ | `asset/vanilla-cache` | `--vanilla-cache` |
| ソースキャッシュ | `asset/source-cache` | `--source-cache` |
| マージ結果 | `asset/merged` | `--merged` |
| 配置先 | `asset/merged_mlo` | `-o` |

**source-cache の生成は、vanilla と同名の YMAP/YBN をリソースの
ストリームディレクトリからキャッシュへ移動します。** 移動後もマージに利用でき、
vanilla に対応しないファイルは元のリソースに残ります。
リソース名はグループをまたいで一意にしてください。
入力と生成物のディレクトリが重なる指定は拒否します。

パイプラインの `-f` は派生キャッシュ、マージ、配置を強制更新しますが、
既存の生 vanilla アーカイブは再利用します。
パイプライン全体の `--gamebuild` と `--step-name` は未実装です。

## 操作を個別に実行する

```bash
mlo_merger --generate-vanilla -i /mnt/gtav -o asset/vanilla
mlo_merger --generate-vanilla-cache --vanilla asset/vanilla -o asset/vanilla-cache
mlo_merger --generate-source-cache -i asset/source --vanilla-cache asset/vanilla-cache -o asset/source-cache
mlo_merger --generate-source-cache resourceName -i asset/source -f
mlo_merger --merge --vanilla-cache asset/vanilla-cache --source-cache asset/source-cache -o asset/merged
mlo_merger --deploy -i asset/merged --source-cache asset/source-cache -o asset/merged_mlo
```

source-cache の対象は、リソース名または入力ディレクトリからの相対パスで指定できます。
`-f` が強制更新するのは選択した対象で、前提ステージではありません。
置き換えたキャッシュは `_old/` に保存します。
キャッシュ内の配置は `resources/{resourceName}/{stream-relative-path}` で、
先頭の `stream/` または `streams/` は含みません。

マージ結果は `ymap/` と `ybn/` に出力します。
置き換えるソースパスは `_omit.txt`、競合する YMAP エンティティの変更は
`duplicates.json` に記録します。変更や破損のあるグループだけ再生成し、
マージ失敗時は直前の出力を保持します。

配置操作は、既存のマージ結果と source-cache を読み、再生成せずに配置します。

- マージ済み: `stream/{extension}/merged/{filename}`。
- 残りのキャッシュ: `stream/{extension}/clone/{resourceName}/{stream-relative-path}`。
- `files.txt`: キャッシュへ移動済みで、現在も有効なファイルの元ソース相対パス。

変更のないコピーは省略し、欠損・変更された出力を修復します。
不要になった管理対象ファイルは削除しますが、無関係なファイルは保持します。
削除対象がローカルで変更されていた場合は、削除せずエラーにします。
空になったストリームディレクトリも整理します。

各コマンドのオプションと出力先の既定値は `mlo_merger --help` で確認できます。
vanilla の抽出範囲を限定する場合は、明示的な生成コマンドの `--gamebuild` で
ステージを指定します。数値の GTA ビルド ID への対応付けは未実装です。
短縮コマンドは [.cargo/config.toml](../.cargo/config.toml) にあります。

## 照合の許容値を設定する

マージと diff-cache 処理は、操作開始時に [asset/config.toml](../asset/config.toml)
を読み込みます。ファイルや項目がない場合は、次の既定値を使います。

```toml
[tolerance]
ybn = 0.05
ymap = 0.001
ymap_occlude_model = 0.01
ymap_box_occluder = 1
```

`ybn` は Box に限らず、対応する全ポリゴンの各軸座標と半径に使い、境界を含みます。
`ymap` は車両生成と LOD ライトの浮動小数点属性に使います。
遮蔽モデルは XY 範囲の許容値、Box 遮蔽形状は保存座標の整数単位を使います。
YMAP の比較では境界を含みません。エンティティ座標の完全一致と、既存の
位置キーの丸めは変更しません。検索の半径とは別の設定です。

値はすべて正数で、浮動小数点値は有限である必要があります。
未定義のキーや不正な TOML はエラーにします。
実効設定をマージの鮮度判定に含めるため、変更後の次回マージで再生成され、
`-f` は不要です。大きな許容値では、意図的に異なる近接形状も同一と判定され得ます。

## 既存データを検索する

```bash
mlo_merger --find entity-guid 2443198849 --filter '*cs4_10_strm_0.ymap' --diff-all
mlo_merger --find entity-position 3.5,4.2,0.0 --round 1.0
mlo_merger --find ybn-position 1211.126,-507.5948,67.54723 --radius 1.0 --type box --filter id2_21_c_0.ybn --diff-all
```

検索はパイプラインを実行せず、生成物も変更せずに XML を標準出力へ返します。
`-i` はマージ結果のディレクトリ、`--filter` は大文字・小文字を区別しない
ファイル名 glob です。ワイルドカードはシェルで展開されないよう引用符で囲みます。
重複する一致結果もすべて保持します。

位置検索は 3D の半径境界を含み、半径の既定値は `1.0` です。
負の座標も指定できますが、非有限・範囲外の座標、負や非有限の半径は拒否します。
エンティティ検索の半径 `0` はネイティブ座標精度での完全一致です。
YBN は形状内部の任意の点ではなく、ワールド座標の形状中心で検索します。
`--type` は `box`、`triangle`、`sphere`、`capsule`、`cylinder` に対応し、
省略時は対応する全形状を検索します。未対応の形状は検索対象外です。

`--diff-all` は、マージ記録にある vanilla と各ソースの入力も表示します。
リソース・パスを併記し、一致のないステージは `found="false"` にします。
記録後に入力が変わっていれば、再マージを求めるエラーになります。
これはスナップショットの比較用出力で、計算済みの意味的差分ではありません。
YBN の番号は入力ごとの値で、再構築すると変わる場合があります。

## ファイルを変換する

```bash
mlo_merger --to-xml collision.ybn -o exported
mlo_merger --from-xml exported/collision.ybn.xml -o rebuilt
mlo_merger --from-xml map.ymap.xml -o rebuilt --vanilla asset/vanilla
```

入力にはファイルまたはディレクトリを指定できます。
出力先の既定値はカレントディレクトリです。
META リソースには互換スキーマが必要で、`--vanilla` はスキーマやテンプレートの
入力に使います。YBN の変換には外部スキーマディレクトリは不要です。

## 制限

- `--get-diff` は現在、模擬呼び出しの表示のみでファイルを比較しません。
- vanilla の履歴はアーカイブの上書き順であり、過去のゲーム環境や完全な
  ゲームエンジンのマウント規則を再現するものではありません。
  マージは選択範囲内の最新 vanilla を使い、過去のベースライン推定は行いません。
- 意味的差分レポートは、履歴を完全に復元できるパッチではありません。
- 変換は対応形式とスキーマに依存します。PSO 編集では配列や文字列領域を
  拡張できず、RBF YMT の再構築は未対応です。
- 変換の成功はゲーム内の安全性を保証しません。GTA V/FiveM 上でも検証してください。

ゲーム資産と CodeWalker のバイナリは同梱していません。
