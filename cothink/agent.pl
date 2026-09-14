#!/usr/bin/perl
use strict;
use warnings;

my $id = $ARGV[0] || 42;
my $file_content = "";
while (<STDIN>) {
    $file_content .= $_;
}
my $content_length = length($file_content);
my $raw_val = ($id * 0.07) + ($content_length * 0.01);
print "$id,$raw_val\n";
