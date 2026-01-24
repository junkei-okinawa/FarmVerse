import json
import pandas as pd
import matplotlib.pyplot as plt
from influxdb_client import InfluxDBClient
from datetime import datetime, timedelta
import os

# --- 設定 ---
AGRI_LOGS_PATH = "../data/agri_logs.json"
INFLUXDB_URL = "http://raspberrypi-base.local:8086"
INFLUXDB_TOKEN = "_mlWsEFjn3M0wPNIf3rlpdrNWyFK--QsMVebSk-0VLAKHwZcBdZJTaYMzYDhRVea3AghB05Dmq27FNp9OwgGAg=="
INFLUXDB_ORG = "agri"
INFLUXDB_BUCKET = "balcony"

def load_agri_logs():
    with open(AGRI_LOGS_PATH, 'r', encoding='utf-8') as f:
        data = json.load(f)
    
    logs = []
    for date_str, info in data.items():
        # 日付のパース (YYYY/MM/DD)
        dt = datetime.strptime(date_str, "%Y/%m/%d")
        
        # 収穫量の合計を計算
        harvest_total = 0
        for key in info:
            if isinstance(info[key], dict) and "sum" in info[key]:
                harvest_total += info[key]["sum"]
        
        logs.append({
            "date": dt.date(),
            "harvest": harvest_total,
            "content": info.get("content", "")
        })
    
    return pd.DataFrame(logs).sort_values("date")

def fetch_sensor_data(start_date, end_date):
    client = InfluxDBClient(url=INFLUXDB_URL, token=INFLUXDB_TOKEN, org=INFLUXDB_ORG)
    query_api = client.query_api()
    
    # 気温とTDS(電圧)を取得するクエリ
    # 注: 実際のフィールド名は app.py やこれまでの調査に基づき推測
    # app.py では DataParser.parse_voltage_data などを使用している
    # InfluxDBには 'voltage', 'temperature' などのフィールドで保存されていると想定
    
    query = f'''
    from(bucket: "{INFLUXDB_BUCKET}")
      |> range(start: {start_date}T00:00:00Z, stop: {end_date}T23:59:59Z)
      |> filter(fn: (r) => r["_field"] == "temperature" or r["_field"] == "voltage")
      |> aggregateWindow(every: 1d, fn: mean, createEmpty: false)
      |> yield(name: "mean")
    '''
    print(f"Querying InfluxDB with: {query}")
    try:
        tables = query_api.query(query)
        print(f"Query returned {len(tables)} tables.")
    except Exception as e:
        print(f"Query failed: {e}")
        return pd.DataFrame()
    
    sensor_records = []
    for table in tables:
        for record in table.records:
            sensor_records.append({
                "date": record.get_time().date(),
                "field": record.get_field(),
                "value": record.get_value()
            })
    
    client.close()
    
    if not sensor_records:
        return pd.DataFrame()
        
    df_sensor = pd.DataFrame(sensor_records)
    # 同一日に複数のデータがある場合（複数のデバイスなど）を考慮して pivot_table を使用
    df_pivot = df_sensor.pivot_table(index="date", columns="field", values="value", aggfunc='mean').reset_index()
    return df_pivot

def main():
    print("日記データを読み込み中...")
    df_logs = load_agri_logs()
    
    if df_logs.empty:
        print("日記データが空です。")
        return

    start_date = df_logs["date"].min()
    end_date = df_logs["date"].max()
    
    print(f"センサーデータを取得中 ({start_date} から {end_date})...")
    df_sensor = fetch_sensor_data(start_date, end_date)
    
    if df_sensor.empty:
        print("センサーデータが取得できませんでした。")
        # センサーデータがない場合でも日記データだけで進めるか、ダミーを表示
    else:
        # データの結合
        df_merged = pd.merge(df_logs, df_sensor, on="date", how="left")
        
        print("分析結果の可視化...")
        fig, ax1 = plt.subplots(figsize=(12, 6))

        # 収穫量の棒グラフ
        ax1.bar(df_merged["date"], df_merged["harvest"], color='green', alpha=0.3, label='Harvest (g)')
        ax1.set_xlabel('Date')
        ax1.set_ylabel('Harvest Amount (g)', color='green')
        ax1.tick_params(axis='y', labelcolor='green')

        # 気温の折れ線グラフ
        ax2 = ax1.twinx()
        if "temperature" in df_merged.columns:
            ax2.plot(df_merged["date"], df_merged["temperature"], color='red', marker='o', label='Avg Temp (°C)')
            ax2.set_ylabel('Temperature (°C)', color='red')
            ax2.tick_params(axis='y', labelcolor='red')

        plt.title('Harvest vs Temperature')
        fig.tight_layout()
        
        output_file = "harvest_analysis.png"
        plt.savefig(output_file)
        print(f"グラフを保存しました: {output_file}")

        # 相関の計算
        if "temperature" in df_merged.columns:
            correlation = df_merged["harvest"].corr(df_merged["temperature"])
            print(f"収穫量と平均気温の相関計数: {correlation:.2f}")

if __name__ == "__main__":
    main()
