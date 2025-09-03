# 🎠 Carousel

A Kubernetes node lifecycle management tool that automatically rotates cluster nodes and pods to ensure fresh, patched infrastructure.

## Overview

Carousel is designed for dynamic Kubernetes clusters with autoscaling capabilities. It helps maintain cluster security by automatically cycling out older nodes and pods, encouraging the autoscaler to provision fresh, up-to-date replacements with the latest patches.

## Features

- **Automated Node Rotation**: Identifies and cordons nodes older than a configurable age (default: 7 days)
- **Pod Lifecycle Management**: Removes stale pods older than a configurable age (default: 7 days)
- **Smart Eviction Strategy**: 
  - Gracefully evicts pods when possible
  - Force deletes pods with local storage (emptyDir volumes)
- **Cluster Health Awareness**: Skips rotation when nodes are unschedulable or not ready
- **Provider-Specific Targeting**: Only manages nodes from specific providers (currently libvirt)
- **Comprehensive Logging**: Structured logging with configurable levels

## How It Works

Carousel runs continuously in your cluster and performs these operations every 5 minutes:

### Node Management
1. **Discovery**: Lists all nodes in the cluster
2. **Health Check**: Verifies no nodes are unschedulable or not ready
3. **Age Assessment**: Identifies nodes older than `NODE_CULL_DAYS`
4. **Selective Targeting**: Only considers nodes with libvirt provider IDs
5. **Rotation**: Cordons the oldest qualifying node and evicts all its pods

### Pod Management
1. **Enumeration**: Lists all pods across all namespaces
2. **Age Filter**: Identifies pods older than `POD_CULL_DAYS`  
3. **Cleanup**: Deletes the oldest qualifying pods

## Configuration

The behavior is controlled by these constants in `src/main.rs`:

```rust
const POD_CULL_DAYS: u64 = 7;    // Pod maximum age in days
const NODE_CULL_DAYS: u64 = 7;   // Node maximum age in days
```

Logging levels are configured via the `RUST_LOG` environment variable.

## Requirements

- **Kubernetes Cluster**: With RBAC enabled
- **Service Account**: With cluster-admin permissions (see `kubernetes/` directory)
- **Autoscaling**: Dynamic node provisioning capability
- **Provider Integration**: Currently designed for libvirt-based clusters

## Deployment

### Using Kustomize (Recommended)

```bash
# Deploy to the 'autoscaler' namespace
kubectl apply -k kubernetes/
```

### Manual Deployment

1. **Create Service Account**:
   ```bash
   kubectl apply -f kubernetes/svcacct.yaml
   kubectl apply -f kubernetes/crb.yaml
   ```

2. **Deploy Application**:
   ```bash
   kubectl apply -f kubernetes/deployment.yaml
   ```

## Building

### Local Development
```bash
cargo build --release
```

### Container Image
```bash
docker build -t carousel:latest .
```

The project includes a `justfile` for common operations:
```bash
just build    # Build the project
just run      # Run locally  
just docker   # Build container image
```

## Safety Features

- **Non-Disruptive**: Won't drain nodes if any nodes are already unschedulable or not ready
- **Provider Scoped**: Only manages nodes from specified providers (libvirt)
- **Graceful Handling**: Prefers pod eviction over deletion when possible
- **Comprehensive Logging**: All operations are logged for audit and debugging

## Use Case

Carousel is ideal for:
- **Homelab Clusters**: With dynamic libvirt-based autoscaling
- **Security-Conscious Environments**: Requiring regular node refresh cycles
- **Development Clusters**: Where infrastructure currency is important
- **Hybrid Environments**: Mixed static/dynamic node configurations

## Version

Current version: **0.1.4**

## License

This project follows standard Rust/Cargo conventions. See `Cargo.toml` for details.