-- Schema iniziale del magazzino (spec 007).

-- ---------------------------------------------------------------- Mappa del magazzino
CREATE TABLE warehouses (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    code        text NOT NULL UNIQUE,
    name        text NOT NULL,
    address     text,
    active      boolean NOT NULL DEFAULT true,
    version     integer NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE zones (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    warehouse_id  uuid NOT NULL REFERENCES warehouses(id),
    code          text NOT NULL,
    name          text NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (warehouse_id, code),
    UNIQUE (id, warehouse_id)
);

CREATE TABLE locations (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    warehouse_id   uuid NOT NULL REFERENCES warehouses(id),
    zone_id        uuid NOT NULL,
    code           text NOT NULL,
    aisle          text,
    shelf          text,
    level          text,
    storage_type   text NOT NULL CHECK (storage_type IN ('AMBIENT','REFRIGERATED','FROZEN','HAZARDOUS','BULK')),
    max_weight_kg  numeric(18,3) CHECK (max_weight_kg > 0),
    max_volume_m3  numeric(18,6) CHECK (max_volume_m3 > 0),
    active         boolean NOT NULL DEFAULT true,
    version        integer NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    UNIQUE (warehouse_id, code),
    -- La zona deve appartenere allo stesso magazzino dell'ubicazione.
    FOREIGN KEY (zone_id, warehouse_id) REFERENCES zones(id, warehouse_id)
);

-- ---------------------------------------------------------------- Anagrafiche
CREATE TABLE products (
    id                     uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    sku                    text NOT NULL UNIQUE,
    ean                    text UNIQUE,
    name                   text NOT NULL,
    description            text,
    category               text,
    uom                    text NOT NULL CHECK (uom IN ('PCS','KG','M','L')),
    unit_price             numeric(18,4) NOT NULL DEFAULT 0 CHECK (unit_price >= 0),
    currency               char(3) NOT NULL DEFAULT 'EUR',
    min_stock              numeric(18,3) NOT NULL DEFAULT 0 CHECK (min_stock >= 0),
    tracking               text NOT NULL DEFAULT 'NONE' CHECK (tracking IN ('NONE','LOT','SERIAL')),
    requires_expiry        boolean NOT NULL DEFAULT false,
    expiry_warning_days    integer NOT NULL DEFAULT 30 CHECK (expiry_warning_days >= 0),
    unit_weight_kg         numeric(18,6) NOT NULL DEFAULT 0 CHECK (unit_weight_kg >= 0),
    unit_volume_m3         numeric(18,6) NOT NULL DEFAULT 0 CHECK (unit_volume_m3 >= 0),
    required_storage_type  text CHECK (required_storage_type IN ('AMBIENT','REFRIGERATED','FROZEN','HAZARDOUS','BULK')),
    status                 text NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE','INACTIVE')),
    version                integer NOT NULL DEFAULT 1,
    created_at             timestamptz NOT NULL DEFAULT now(),
    updated_at             timestamptz NOT NULL DEFAULT now(),
    CHECK (NOT requires_expiry OR tracking <> 'NONE')
);
CREATE INDEX products_category_idx ON products (lower(category));

CREATE TABLE lots (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id       uuid NOT NULL REFERENCES products(id),
    lot_code         text NOT NULL,
    production_date  date,
    expiry_date      date,
    received_at      timestamptz NOT NULL DEFAULT now(),
    status           text NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE','BLOCKED')),
    created_at       timestamptz NOT NULL DEFAULT now(),
    UNIQUE (product_id, lot_code)
);
CREATE INDEX lots_expiry_idx ON lots (expiry_date);

CREATE TABLE serials (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id     uuid NOT NULL REFERENCES products(id),
    serial_number  text NOT NULL,
    lot_id         uuid REFERENCES lots(id),
    location_id    uuid REFERENCES locations(id),
    status         text NOT NULL CHECK (status IN ('IN_STOCK','SHIPPED','SCRAPPED')),
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    UNIQUE (product_id, serial_number),
    CHECK ((status = 'IN_STOCK') = (location_id IS NOT NULL))
);

-- ---------------------------------------------------------------- Giacenze
-- Una riga per (prodotto, lotto, ubicazione) con la sola quantità fisica: l'impegno è soft, per
-- prodotto e magazzino, ed è la somma delle righe d'ordine di vendita (spec 007).
CREATE TABLE stock (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    warehouse_id  uuid NOT NULL REFERENCES warehouses(id),
    location_id   uuid NOT NULL REFERENCES locations(id),
    product_id    uuid NOT NULL REFERENCES products(id),
    lot_id        uuid REFERENCES lots(id),
    quantity      numeric(18,3) NOT NULL CHECK (quantity >= 0),
    received_at   timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE NULLS NOT DISTINCT (product_id, lot_id, location_id)
);
CREATE INDEX stock_product_wh_idx ON stock (product_id, warehouse_id);
CREATE INDEX stock_location_idx ON stock (location_id);

-- ---------------------------------------------------------------- Ordini
CREATE SEQUENCE sales_order_number_seq;
CREATE SEQUENCE purchase_order_number_seq;

CREATE TABLE sales_orders (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    number         text NOT NULL UNIQUE,
    external_ref   text,
    customer_name  text NOT NULL,
    warehouse_id   uuid NOT NULL REFERENCES warehouses(id),
    status         text NOT NULL DEFAULT 'OPEN'
                   CHECK (status IN ('OPEN','PARTIALLY_RESERVED','RESERVED','PARTIALLY_FULFILLED','FULFILLED','CANCELLED')),
    notes          text,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sales_order_lines (
    id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id            uuid NOT NULL REFERENCES sales_orders(id),
    line_no             integer NOT NULL,
    product_id          uuid NOT NULL REFERENCES products(id),
    quantity_ordered    numeric(18,3) NOT NULL CHECK (quantity_ordered > 0),
    quantity_reserved   numeric(18,3) NOT NULL DEFAULT 0 CHECK (quantity_reserved >= 0),
    quantity_fulfilled  numeric(18,3) NOT NULL DEFAULT 0 CHECK (quantity_fulfilled >= 0),
    UNIQUE (order_id, line_no),
    CHECK (quantity_reserved + quantity_fulfilled <= quantity_ordered)
);
CREATE INDEX sales_order_lines_product_idx ON sales_order_lines (product_id) WHERE quantity_reserved > 0;

CREATE TABLE purchase_orders (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    number         text NOT NULL UNIQUE,
    external_ref   text,
    supplier_name  text NOT NULL,
    warehouse_id   uuid NOT NULL REFERENCES warehouses(id),
    status         text NOT NULL DEFAULT 'OPEN' CHECK (status IN ('OPEN','PARTIALLY_RECEIVED','RECEIVED','CANCELLED')),
    expected_date  date,
    notes          text,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE purchase_order_lines (
    id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id           uuid NOT NULL REFERENCES purchase_orders(id),
    line_no            integer NOT NULL,
    product_id         uuid NOT NULL REFERENCES products(id),
    quantity_ordered   numeric(18,3) NOT NULL CHECK (quantity_ordered > 0),
    quantity_received  numeric(18,3) NOT NULL DEFAULT 0 CHECK (quantity_received >= 0),
    UNIQUE (order_id, line_no),
    CHECK (quantity_received <= quantity_ordered)
);

-- ---------------------------------------------------------------- Movimenti (immutabili)
CREATE TABLE movements (
    id                      uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    number                  bigint GENERATED ALWAYS AS IDENTITY UNIQUE,
    operation_id            uuid NOT NULL,
    type                    text NOT NULL CHECK (type IN ('INBOUND','OUTBOUND','TRANSFER','ADJUSTMENT','RESERVATION','RELEASE')),
    direction               text CHECK (direction IN ('IN','OUT')),
    product_id              uuid NOT NULL REFERENCES products(id),
    lot_id                  uuid REFERENCES lots(id),
    warehouse_id            uuid NOT NULL REFERENCES warehouses(id),
    from_location_id        uuid REFERENCES locations(id),
    to_location_id          uuid REFERENCES locations(id),
    quantity                numeric(18,3) NOT NULL CHECK (quantity > 0),
    reason_code             text NOT NULL,
    note                    text,
    sales_order_id          uuid REFERENCES sales_orders(id),
    sales_order_line_id     uuid REFERENCES sales_order_lines(id),
    purchase_order_id       uuid REFERENCES purchase_orders(id),
    purchase_order_line_id  uuid REFERENCES purchase_order_lines(id),
    external_ref            text,
    operator                text NOT NULL,
    occurred_at             timestamptz NOT NULL DEFAULT now(),
    created_at              timestamptz NOT NULL DEFAULT now(),
    reverses_movement_id    uuid UNIQUE REFERENCES movements(id),
    fefo_override_reason    text,
    CHECK ((type = 'ADJUSTMENT') = (direction IS NOT NULL))
);
CREATE INDEX movements_product_idx ON movements (product_id, occurred_at DESC);
CREATE INDEX movements_lot_idx ON movements (lot_id);
CREATE INDEX movements_from_idx ON movements (from_location_id);
CREATE INDEX movements_to_idx ON movements (to_location_id);
CREATE INDEX movements_so_idx ON movements (sales_order_id);
CREATE INDEX movements_po_idx ON movements (purchase_order_id);
CREATE INDEX movements_operation_idx ON movements (operation_id);

CREATE TABLE movement_serials (
    movement_id  uuid NOT NULL REFERENCES movements(id),
    serial_id    uuid NOT NULL REFERENCES serials(id),
    PRIMARY KEY (movement_id, serial_id)
);
CREATE INDEX movement_serials_serial_idx ON movement_serials (serial_id);

-- Regola 4: un movimento confermato non si modifica né si elimina, nemmeno da SQL.
CREATE FUNCTION forbid_movement_change() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'I movimenti di magazzino sono immutabili: usare uno storno'
        USING ERRCODE = 'check_violation';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER movements_immutable
    BEFORE UPDATE OR DELETE ON movements
    FOR EACH ROW EXECUTE FUNCTION forbid_movement_change();
CREATE TRIGGER movements_no_truncate
    BEFORE TRUNCATE ON movements
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_movement_change();
CREATE TRIGGER movement_serials_immutable
    BEFORE UPDATE OR DELETE ON movement_serials
    FOR EACH ROW EXECUTE FUNCTION forbid_movement_change();

-- ---------------------------------------------------------------- Idempotenza
CREATE TABLE idempotency_keys (
    key            text PRIMARY KEY,
    method         text NOT NULL,
    path           text NOT NULL,
    request_hash   text NOT NULL,
    status_code    integer,
    response_body  bytea,
    created_at     timestamptz NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------- Disponibilità
-- Situazione per (prodotto, magazzino): available = physical − unusable − reserved (spec 007).
-- Un lotto è inutilizzabile se bloccato o scaduto (scadenza anteriore a oggi).
CREATE VIEW product_warehouse_figures AS
WITH s AS (
    SELECT s.product_id, s.warehouse_id,
           sum(s.quantity) AS physical,
           COALESCE(sum(s.quantity) FILTER (WHERE l.status = 'BLOCKED' OR l.expiry_date < CURRENT_DATE), 0) AS unusable,
           COALESCE(sum(s.quantity) FILTER (WHERE l.expiry_date < CURRENT_DATE), 0) AS expired,
           COALESCE(sum(s.quantity) FILTER (WHERE l.expiry_date >= CURRENT_DATE
                                              AND l.expiry_date <= CURRENT_DATE + p.expiry_warning_days), 0) AS expiring
    FROM stock s
    JOIN products p ON p.id = s.product_id
    LEFT JOIN lots l ON l.id = s.lot_id
    GROUP BY s.product_id, s.warehouse_id
), r AS (
    SELECT sol.product_id, so.warehouse_id, sum(sol.quantity_reserved) AS reserved
    FROM sales_order_lines sol
    JOIN sales_orders so ON so.id = sol.order_id
    WHERE sol.quantity_reserved > 0
    GROUP BY sol.product_id, so.warehouse_id
)
SELECT COALESCE(s.product_id, r.product_id) AS product_id,
       COALESCE(s.warehouse_id, r.warehouse_id) AS warehouse_id,
       COALESCE(s.physical, 0) AS physical,
       COALESCE(s.unusable, 0) AS unusable,
       COALESCE(r.reserved, 0) AS reserved,
       COALESCE(s.physical, 0) - COALESCE(s.unusable, 0) - COALESCE(r.reserved, 0) AS available,
       COALESCE(s.expiring, 0) AS expiring,
       COALESCE(s.expired, 0) AS expired
FROM s FULL JOIN r ON r.product_id = s.product_id AND r.warehouse_id = s.warehouse_id;
