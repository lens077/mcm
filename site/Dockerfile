# 落地页容器：构建静态产物，用 nginx 托管。
#
# Pangolin 是隧道反向代理，本身不托管文件——它把流量转发到某台已连接的机器。
# 所以「上线」的实际形态是：在目标机器上跑这个容器，再在 Pangolin 里
# 把域名指向它的端口。
#
#   docker build -t mcm-site site/
#   docker run -d -p 8080:80 --name mcm-site mcm-site
#
# 之后在该机器安装 Newt 并连入 Pangolin，把资源指向 127.0.0.1:8080 即可。

FROM node:24-alpine AS build
WORKDIR /app

# 先装依赖再拷源码，依赖没变时可命中缓存
COPY package.json pnpm-lock.yaml* ./
RUN corepack enable && pnpm install --no-frozen-lockfile

COPY . .
RUN pnpm build

FROM nginx:alpine
# 站点是纯静态的，默认配置即可；这里只替换内容目录
COPY --from=build /app/dist /usr/share/nginx/html

# 静态资源带哈希文件名，可长缓存；HTML 不缓存以便发布即时生效
RUN printf 'server {\n\
  listen 80;\n\
  root /usr/share/nginx/html;\n\
  index index.html;\n\
  location /_astro/ { expires 1y; add_header Cache-Control "public, immutable"; }\n\
  location / { try_files $uri $uri/ /index.html; add_header Cache-Control "no-cache"; }\n\
}\n' > /etc/nginx/conf.d/default.conf

EXPOSE 80
