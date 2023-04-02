# Build a stateful microservice with Dapr

## Prerequisite

[Install and start the MySQL database](https://dev.mysql.com/doc/mysql-installation-excerpt/8.0/en/)

Install and init Dapr CLI.

```bash
wget -q https://raw.githubusercontent.com/dapr/cli/master/install/install.sh -O - | /bin/bash
dapr init
```

## Build

```bash
cd sales_tax_rate
cargo build --target wasm32-wasi --release

cd order_management
cargo build --target wasm32-wasi --release
```

## Run

```bash
cd sales_tax_rate
dapr run --app-id rate-service --app-protocol http --app-port 8001 --dapr-http-port 3501 --components-path ../config --log-level debug wasmedge target/wasm32-wasi/release/sales_tax_rate_lookup.wasm

cd order_management
dapr run --app-id order-service --app-protocol http --app-port 8003 --dapr-http-port 3503 --components-path ../config --log-level debug wasmedge target/wasm32-wasi/release/order_management.wasm
```

## Test

Run the following from another terminal.

```bash
$ curl http://localhost:3503/invoke/order-service/method/init
{"status":"true"}

$ curl http://localhost:3503/invoke/order-service/method/create_order -X POST -d @order.json
{
  "order_id": 0,
  "product_id": 321,
  "quantity": 2,
  "subtotal": 20.0,
  "shipping_address": "123 Main St, Anytown USA",
  "shipping_zip": "78701",
  "shipping_cost": 5.5,
  "total": 27.15
}

$ curl http://localhost:3503/invoke/order-service/method/orders
[{"order_id":1,"product_id":321,"quantity":2,"subtotal":20.0,"shipping_address":"123 Main St, Anytown USA","shipping_zip":"78701","shipping_cost":5.5,"total":27.15}]
```
