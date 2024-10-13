# Rationale
I have a kubernetes cluster in my homelab that has dynamic autoscaling based on libvirt.  For security reasons, I'd like the nodes to be recent on patches.
As a result, I'd like to encourage the autoscaler to self-roll nodes that are older than a certain age.

# Design ideas
Rough thoughts:
  - config file which specifies how old the nodes should be
  - we'll use kubernetes api, so we'll need a story for getting the auth token (service account passthru)

