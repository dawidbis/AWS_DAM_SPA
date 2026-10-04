# Tabela `assets` (rozdział 5). Statusy zmieniane są wyłącznie warunkowymi
# zapisami, a indeksy obsługują kolejkę publikacji / panel kwarantanny
# (status-index) i widok „moje zgłoszenia” (uploader-index).
# Stan uploadu multipart (uploadId, rozmiar części) jest zapisany przy assecie,
# więc osobna tabela `uploads` nie jest potrzebna.

terraform {
  required_version = ">= 1.10"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 6.0"
    }
  }
}

resource "aws_dynamodb_table" "assets" {
  #checkov:skip=CKV_AWS_119:Szyfrowanie kluczem należącym do AWS (bez kosztu KMS CMK, rozdział 7.3)
  name                        = "${var.name_prefix}-assets"
  billing_mode                = "PAY_PER_REQUEST"
  hash_key                    = "pk"
  deletion_protection_enabled = var.deletion_protection

  attribute {
    name = "pk"
    type = "S"
  }

  attribute {
    name = "status"
    type = "S"
  }

  attribute {
    name = "uploaderId"
    type = "S"
  }

  attribute {
    name = "createdAt"
    type = "N"
  }

  global_secondary_index {
    name            = "status-index"
    projection_type = "ALL"

    key_schema {
      attribute_name = "status"
      key_type       = "HASH"
    }

    key_schema {
      attribute_name = "createdAt"
      key_type       = "RANGE"
    }
  }

  global_secondary_index {
    name            = "uploader-index"
    projection_type = "ALL"

    key_schema {
      attribute_name = "uploaderId"
      key_type       = "HASH"
    }

    key_schema {
      attribute_name = "createdAt"
      key_type       = "RANGE"
    }
  }

  point_in_time_recovery {
    enabled = true
  }

  server_side_encryption {
    enabled = false # false = klucz należący do AWS (domyślne szyfrowanie)
  }
}

# Incydenty bezpieczeństwa (rozdział 5): jeden wpis na zainfekowany asset,
# zapisywany przez handle-infected. Dowód z adresem IP i sygnaturą.
resource "aws_dynamodb_table" "incidents" {
  #checkov:skip=CKV_AWS_119:Szyfrowanie kluczem należącym do AWS (bez kosztu KMS CMK, rozdział 7.3)
  name                        = "${var.name_prefix}-incidents"
  billing_mode                = "PAY_PER_REQUEST"
  hash_key                    = "incidentId"
  deletion_protection_enabled = var.deletion_protection

  attribute {
    name = "incidentId"
    type = "S"
  }

  point_in_time_recovery {
    enabled = true
  }

  server_side_encryption {
    enabled = false # false = klucz należący do AWS (domyślne szyfrowanie)
  }
}

# Słowniki klubu (etap 3): zawodnicy, sezony, rozgrywki, mecze, sponsorzy.
# Klucz partycji `kind` (PLAYER, SEASON, …), klucz sortowania `id` (slug),
# wpis jako JSON w atrybucie `data` (shared::dictionary). Kilkadziesiąt
# rekordów, więc lista to Query po `kind` albo Scan całej tabeli.
resource "aws_dynamodb_table" "dictionaries" {
  #checkov:skip=CKV_AWS_119:Szyfrowanie kluczem należącym do AWS (bez kosztu KMS CMK, rozdział 7.3)
  name                        = "${var.name_prefix}-dictionaries"
  billing_mode                = "PAY_PER_REQUEST"
  hash_key                    = "kind"
  range_key                   = "id"
  deletion_protection_enabled = var.deletion_protection

  attribute {
    name = "kind"
    type = "S"
  }

  attribute {
    name = "id"
    type = "S"
  }

  point_in_time_recovery {
    enabled = true
  }

  server_side_encryption {
    enabled = false # false = klucz należący do AWS (domyślne szyfrowanie)
  }
}
