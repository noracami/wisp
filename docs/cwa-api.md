# CWA (中央氣象署) API 參考

## 使用的 API

**一般天氣預報 - 今明 36 小時天氣預報**

- Endpoint: `GET https://opendata.cwa.gov.tw/api/v1/rest/datastore/F-C0032-001`
- 文件: https://opendata.cwa.gov.tw/dist/opendata-swagger.html#/%E9%A0%90%E5%A0%B1/get_v1_rest_datastore_F_C0032_001

## 認證

- Query parameter: `Authorization=<CWA_API_KEY>`
- API key 申請: https://opendata.cwa.gov.tw/userLogin

## 查詢參數

| 參數 | 說明 |
|------|------|
| `Authorization` | API key (必填) |
| `locationName` | 地區名稱，必須使用正體中文全名 (見下方列表) |

## 有效的 locationName 值

CWA API **只接受正體中文全名**，使用「台」(簡體) 或省略「市/縣」後綴都會回傳空結果。

```
嘉義縣  新北市  嘉義市  新竹縣  新竹市
臺北市  臺南市  宜蘭縣  苗栗縣  雲林縣
花蓮縣  臺中市  臺東縣  桃園市  南投縣
高雄市  金門縣  屏東縣  基隆市  澎湖縣
彰化縣  連江縣
```

注意: `臺北市` (正體) 可以查到，`台北市` (簡體台) 查不到。

## 回應結構

```json
{
  "success": "true",
  "records": {
    "location": [
      {
        "locationName": "臺北市",
        "weatherElement": [
          {
            "elementName": "Wx",        // 天氣現象
            "time": [
              {
                "startTime": "2026-03-25 06:00:00",
                "endTime": "2026-03-25 18:00:00",
                "parameter": {
                  "parameterName": "晴時多雲",
                  "parameterValue": "2"
                }
              }
            ]
          },
          { "elementName": "PoP" },     // 降雨機率 (%)
          { "elementName": "MinT" },    // 最低溫度 (°C)
          { "elementName": "MaxT" },    // 最高溫度 (°C)
          { "elementName": "CI" }       // 舒適度指數
        ]
      }
    ]
  }
}
```

## 注意事項

- 每次回傳 3 個時段 (36 小時內)
- `locationName` 查詢不到時不會報錯，而是回傳空的 `location` 陣列
- 程式碼中需做 location alias 對照，將常見的簡寫/簡體字對應到 API 要求的正體全名 (見 `src/weather/cwa.rs`)
