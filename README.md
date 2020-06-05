<h1 align="center">
  <code>Furumi</code>
</h1>
<div align="center">
  <strong>
    FUSE filesystem working over NGINX JSON autoindex. <br />
    Written in Rust stable.
  </strong>
    <br />
  <img src="https://github.com/house-of-vanity/furumi/workflows/Build%20and%20publish/badge.svg" alt="Build and publish"/>
</div>





# Features
  - Using NGINX for indexing remote files.
  - Security relies on HTTPS.
  - Using cache.
  
## Usage
Here is a [binary release](https://github.com/house-of-vanity/furumi/releases/latest) or compile it yourself. Anyway mind about dependencies listed below. Also there is a systemd unit file for managing service. Place it into `~/.config/systemd/user/furumi.service`

```sh
# Compile binary
$ cargo build --release

# Create config
cat > furumi.ylm <<EOF
---
server: https://server
mountpoint: /mnt
# Basic auth creds
username: user
password: pass

# Run
$ ./target/release/furumi --conf furumi.yml

```

## NGINX config
Example of nginx config:
```nginx
server {
    listen 80;
    listen [::]:80;
    server_name music;

    root /storage/music;

    location / {
        autoindex on;
        autoindex_format json;
        try_files $uri $uri/ =404;
    }
}
```

## Dependencies

FUSE must be installed to build and run furumi. (i.e. kernel driver and libraries. Some platforms may also require userland utils like `fusermount`). A default installation of FUSE is usually sufficient.

### Linux

[FUSE for Linux][libfuse] is available in most Linux distributions and usually called `fuse`. 

Install on Arch Linux:

```sh
sudo pacman -S fuse
```

Install on Debian based system:

```sh
sudo apt-get install fuse
```

Install on CentOS:

```sh
sudo yum install fuse
```

To build, FUSE libraries and headers are required. The package is usually called `libfuse-dev` or `fuse-devel`. Also `pkg-config` is required for locating libraries and headers.

```sh
sudo apt-get install libfuse-dev pkg-config
```

```sh
sudo yum install fuse-devel pkgconfig
```


