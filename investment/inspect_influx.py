from influxdb_client import InfluxDBClient
import os

# --- 設定 ---
INFLUXDB_URL = "http://raspberrypi-base.local:8086"
INFLUXDB_TOKEN = "_mlWsEFjn3M0wPNIf3rlpdrNWyFK--QsMVebSk-0VLAKHwZcBdZJTaYMzYDhRVea3AghB05Dmq27FNp9OwgGAg=="
INFLUXDB_ORG = "agri"
INFLUXDB_BUCKET = "balcony"

def inspect_bucket():
    client = InfluxDBClient(url=INFLUXDB_URL, token=INFLUXDB_TOKEN, org=INFLUXDB_ORG)
    query_api = client.query_api()
    
    print(f"--- Bucket: {INFLUXDB_BUCKET} のデータ構造を確認中 ---")
    
    # 1. メジャーメント（Measurement）の取得
    query_measurements = f'import "influxdata/influxdb/schema"; schema.measurements(bucket: "{INFLUXDB_BUCKET}")'
    try:
        tables = query_api.query(query_measurements)
        measurements = [record.get_value() for table in tables for record in table.records]
        print(f"Measurements: {measurements}")
        
        for m in measurements:
            print(f"\nMeasurement '{m}' のフィールド:")
            # 2. 各メジャーメントのフィールド（Field Key）の取得
            query_fields = f'import "influxdata/influxdb/schema"; schema.measurementFieldKeys(bucket: "{INFLUXDB_BUCKET}", measurement: "{m}")'
            field_tables = query_api.query(query_fields)
            fields = [record.get_value() for table in field_tables for record in table.records]
            print(f"  Fields: {fields}")
            
            # 3. タグ（Tag Key）の取得
            query_tags = f'import "influxdata/influxdb/schema"; schema.measurementTagKeys(bucket: "{INFLUXDB_BUCKET}", measurement: "{m}")'
            tag_tables = query_api.query(query_tags)
            tags = [record.get_value() for table in tag_tables for record in table.records]
            print(f"  Tags: {tags}")

            # 4. 最新のデータサンプルを1件取得
            query_sample = f'from(bucket: "{INFLUXDB_BUCKET}") |> range(start: -30d) |> filter(fn: (r) => r._measurement == "{m}") |> last()'
            sample_tables = query_api.query(query_sample)
            if sample_tables:
                print(f"  最新データサンプル:")
                for table in sample_tables:
                    for record in table.records:
                        print(f"    {record.get_field()}: {record.get_value()} (at {record.get_time()})")

    except Exception as e:
        print(f"Error: {e}")
    finally:
        client.close()

if __name__ == "__main__":
    inspect_bucket()
