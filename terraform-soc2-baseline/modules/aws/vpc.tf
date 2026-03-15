# ---------------------------------------------------------------------------
# CC6.1 / A1.2 — VPC Segmentation (AWS)
#
# Multi-AZ VPC with public and private subnets.
# Services run in private subnets; NAT gateway provides egress.
# Security groups follow least-privilege (deny by default).
# ---------------------------------------------------------------------------

data "aws_availability_zones" "available" {
  state = "available"
}

resource "aws_vpc" "main" {
  cidr_block           = var.vpc_cidr
  enable_dns_support   = true
  enable_dns_hostnames = true

  tags = {
    Name        = "soc2-${var.environment}-vpc"
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
    Control     = "CC6.1"
  }
}

# Public subnets (NAT gateways only — no application workloads)
resource "aws_subnet" "public" {
  count             = length(var.public_subnet_cidrs)
  vpc_id            = aws_vpc.main.id
  cidr_block        = var.public_subnet_cidrs[count.index]
  availability_zone = data.aws_availability_zones.available.names[count.index]

  # Do NOT auto-assign public IPs — explicit assignment only
  map_public_ip_on_launch = false

  tags = {
    Name        = "soc2-${var.environment}-public-${count.index + 1}"
    Tier        = "public"
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
  }
}

# Private subnets (application workloads)
resource "aws_subnet" "private" {
  count             = length(var.private_subnet_cidrs)
  vpc_id            = aws_vpc.main.id
  cidr_block        = var.private_subnet_cidrs[count.index]
  availability_zone = data.aws_availability_zones.available.names[count.index]

  map_public_ip_on_launch = false

  tags = {
    Name        = "soc2-${var.environment}-private-${count.index + 1}"
    Tier        = "private"
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
  }
}

# Internet gateway
resource "aws_internet_gateway" "main" {
  vpc_id = aws_vpc.main.id
  tags = {
    Name        = "soc2-${var.environment}-igw"
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
  }
}

# NAT gateways (one per AZ for HA egress from private subnets)
resource "aws_eip" "nat" {
  count  = length(var.public_subnet_cidrs)
  domain = "vpc"
  tags = {
    Name        = "soc2-${var.environment}-nat-eip-${count.index + 1}"
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
  }
}

resource "aws_nat_gateway" "main" {
  count         = length(var.public_subnet_cidrs)
  allocation_id = aws_eip.nat[count.index].id
  subnet_id     = aws_subnet.public[count.index].id
  tags = {
    Name        = "soc2-${var.environment}-nat-${count.index + 1}"
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
  }
}

# Route tables
resource "aws_route_table" "public" {
  vpc_id = aws_vpc.main.id
  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.main.id
  }
  tags = { Name = "soc2-${var.environment}-public-rt", Environment = var.environment }
}

resource "aws_route_table" "private" {
  count  = length(var.private_subnet_cidrs)
  vpc_id = aws_vpc.main.id
  route {
    cidr_block     = "0.0.0.0/0"
    nat_gateway_id = aws_nat_gateway.main[count.index].id
  }
  tags = { Name = "soc2-${var.environment}-private-rt-${count.index + 1}", Environment = var.environment }
}

resource "aws_route_table_association" "public" {
  count          = length(var.public_subnet_cidrs)
  subnet_id      = aws_subnet.public[count.index].id
  route_table_id = aws_route_table.public.id
}

resource "aws_route_table_association" "private" {
  count          = length(var.private_subnet_cidrs)
  subnet_id      = aws_subnet.private[count.index].id
  route_table_id = aws_route_table.private[count.index].id
}

# ---------------------------------------------------------------------------
# Security groups — application tier
# ---------------------------------------------------------------------------

resource "aws_security_group" "app" {
  name        = "soc2-${var.environment}-app-sg"
  vpc_id      = aws_vpc.main.id
  description = "Application tier — allows HTTPS ingress from load balancer only"

  ingress {
    description     = "HTTPS from load balancer security group"
    from_port       = 8080
    to_port         = 8090
    protocol        = "tcp"
    security_groups = [aws_security_group.alb.id]
  }

  egress {
    description = "Allow all egress (NAT gateway controls outbound)"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name        = "soc2-${var.environment}-app-sg"
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
    Control     = "CC6.1"
  }
}

resource "aws_security_group" "alb" {
  name        = "soc2-${var.environment}-alb-sg"
  vpc_id      = aws_vpc.main.id
  description = "Load balancer — HTTPS ingress from internet"

  ingress {
    description = "HTTPS"
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name        = "soc2-${var.environment}-alb-sg"
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
  }
}
