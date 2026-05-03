// OSS appliance — multi-arch Buildx bake (Rust cross-compiled via Zig; BEAM + Debian match each platform).
//
// From repository root (plasm-core / plasm-oss checkout):
//   docker buildx create --name plasm-oss --driver docker-container --use   # once
//   docker buildx bake -f docker/oss-appliance-bake.hcl                   # build manifest locally (builder)
//   docker buildx bake -f docker/oss-appliance-bake.hcl --push              # publish multi-arch to registry
//
// `--load` only supports a single platform; use --set "*.platform=linux/amd64" for local docker load.

variable "TAG" {
  default = "plasm-oss-appliance:local"
}

target "oss-appliance" {
  context    = "."
  dockerfile = "docker/oss-appliance.Dockerfile"
  platforms  = ["linux/amd64", "linux/arm64"]
  tags       = [TAG]
}
